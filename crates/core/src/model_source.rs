//! The DLSS 5 model ships with the games and apps that support DLSS 5, and
//! NVIDIA publishes it nowhere else. So the user's own copy is the source:
//! find the NVIDIA-signed `nvngx_dlssnr.dll` in scanned games and copy the
//! chosen one into our data folder. Nothing is downloaded. Copies we placed
//! ourselves are not sources; they are already the user's chosen file.

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::{
    analysis::{dlss::MODEL_DLL, walk::Tree, WALK_DEPTH},
    components::fetch::sha256_file,
    install::InstallManifest,
    library::Library,
    model_fit::{model_build, nvidia_signed, ModelBuild},
    paths::AppPaths,
    platform::SignatureChecker,
    Error, Result,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ModelSource {
    pub game_id: String,
    pub title: String,
    pub path: String,
    #[ts(type = "number")]
    pub size: u64,
    pub sha256: String,
    pub build: ModelBuild,
}

/// NVIDIA-signed models in the library's games, one per distinct file.
pub fn find(library: &Library, paths: &AppPaths, sig: &dyn SignatureChecker) -> Vec<ModelSource> {
    let ours = placed_by_us(library, paths);
    let mut out: Vec<ModelSource> = Vec::new();
    for game in &library.games {
        if !game.analysis.as_ref().is_some_and(|a| a.has_dlss5_model) {
            continue;
        }
        let tree = Tree::collect(&game.install_dir, WALK_DEPTH);
        let Some(entry) = tree.find_file(MODEL_DLL) else {
            continue;
        };
        if ours.iter().any(|p| p == &entry.path) {
            continue;
        }
        let signature = sig.check(&entry.path);
        if !nvidia_signed(&signature) {
            continue;
        }
        let (Ok(meta), Ok(sha256)) = (std::fs::metadata(&entry.path), sha256_file(&entry.path))
        else {
            continue;
        };
        if out.iter().any(|s| s.sha256 == sha256) {
            continue;
        }
        out.push(ModelSource {
            game_id: game.id.clone(),
            title: game.title.clone(),
            path: entry.path.display().to_string(),
            size: meta.len(),
            build: model_build(&sha256, &signature),
            sha256,
        });
    }
    out
}

/// Every file an install record says we wrote.
fn placed_by_us(library: &Library, paths: &AppPaths) -> Vec<PathBuf> {
    library
        .games
        .iter()
        .filter_map(|g| InstallManifest::load(paths, &g.id).ok().flatten())
        .flat_map(|m| m.files.into_iter().map(|f| f.target))
        .collect()
}

/// Copy a signed model into `user-files\models\<hash>\` and return the copy.
/// The source is checked again here, not trusted from an earlier listing.
pub fn adopt(src: &Path, paths: &AppPaths, sig: &dyn SignatureChecker) -> Result<PathBuf> {
    if !nvidia_signed(&sig.check(src)) {
        return Err(Error::UserFileMissing {
            what: "DLSS 5 model".into(),
            detail: format!(
                "{} does not carry an intact NVIDIA signature",
                src.display()
            ),
        });
    }
    let sha256 = sha256_file(src)?;
    let dir = paths.user_files_dir().join("models").join(&sha256[..16]);
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let dst = dir.join(MODEL_DLL);
    if !dst.is_file() || sha256_file(&dst)? != sha256 {
        std::fs::copy(src, &dst).map_err(|e| Error::io(&dst, e))?;
        if sha256_file(&dst)? != sha256 {
            let _ = std::fs::remove_file(&dst);
            return Err(Error::UserFileMissing {
                what: "DLSS 5 model".into(),
                detail: "the copy does not match the original".into(),
            });
        }
    }
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::walk::testing::make_tree;
    use crate::install::manifest::{FileRecord, INSTALL_SCHEMA};
    use crate::install::planner::testing::game;
    use crate::model::{GraphicsApi, Route};
    use crate::platform::Signature;

    /// Files whose bytes start with "nv" are NVIDIA-signed.
    struct FakeSig;
    impl SignatureChecker for FakeSig {
        fn check(&self, file: &Path) -> Signature {
            match std::fs::read(file) {
                Ok(b) if b.starts_with(b"nv") => Signature::Valid {
                    signer: "NVIDIA Corporation".into(),
                },
                Ok(_) => Signature::HashMismatch {
                    signer: "NVIDIA Corporation".into(),
                },
                Err(e) => Signature::Unknown(e.to_string()),
            }
        }
    }

    fn with_model(dir: &Path, id: &str, title: &str) -> crate::model::Game {
        let mut g = game(dir, &[GraphicsApi::Dx12], Route::ReShadeRenoDx, true);
        g.id = id.into();
        g.title = title.into();
        g.analysis.as_mut().unwrap().has_dlss5_model = true;
        g
    }

    #[test]
    fn lists_signed_models_once_and_skips_modified_and_our_own_copies() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        paths.ensure().unwrap();
        let root = tmp.path();
        make_tree(
            root,
            &[
                ("Ball/Bin/nvngx_dlssnr.dll", b"nv-october"),
                ("Remix/nvngx_dlssnr.dll", b"nv-october"),
                ("Patched/nvngx_dlssnr.dll", b"patched"),
                ("Ours/nvngx_dlssnr.dll", b"nv-ours"),
                ("NoModel/x.exe", b""),
            ],
        );
        let library = Library {
            games: vec![
                with_model(&root.join("Ball"), "b", "Ball"),
                with_model(&root.join("Remix"), "r", "Remix"),
                with_model(&root.join("Patched"), "p", "Patched"),
                with_model(&root.join("Ours"), "o", "Ours"),
                game(
                    &root.join("NoModel"),
                    &[GraphicsApi::Dx12],
                    Route::ReShadeRenoDx,
                    true,
                ),
            ],
            ..Library::default()
        };
        InstallManifest {
            schema: INSTALL_SCHEMA,
            game_id: "o".into(),
            exe: root.join("Ours/G.exe"),
            route: Route::ReShadeRenoDx,
            installed_at: 1,
            components: vec![],
            model_sha256: None,
            user_files: vec![],
            anti_cheat_override: false,
            files: vec![FileRecord {
                target: root.join("Ours/nvngx_dlssnr.dll"),
                backup: None,
                sha256_before: None,
                sha256_after: "x".into(),
            }],
            side_effects: vec![],
        }
        .save(&paths)
        .unwrap();

        let found = find(&library, &paths, &FakeSig);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].title, "Ball");
        assert_eq!(found[0].build, ModelBuild::NvidiaSigned);
        assert_eq!(found[0].size, 10);
    }

    #[test]
    fn adopt_copies_a_signed_model_and_refuses_anything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        make_tree(
            tmp.path(),
            &[
                ("a/nvngx_dlssnr.dll", b"nv-model"),
                ("b/nvngx_dlssnr.dll", b"patched"),
            ],
        );
        let src = tmp.path().join("a/nvngx_dlssnr.dll");
        let copy = adopt(&src, &paths, &FakeSig).unwrap();
        assert!(copy.starts_with(paths.user_files_dir()));
        assert_eq!(std::fs::read(&copy).unwrap(), b"nv-model");
        assert_eq!(adopt(&src, &paths, &FakeSig).unwrap(), copy, "idempotent");

        let err = adopt(&tmp.path().join("b/nvngx_dlssnr.dll"), &paths, &FakeSig).unwrap_err();
        assert_eq!(err.code(), "user_file_missing");
    }
}
