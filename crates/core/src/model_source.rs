//! The DLSS 5 model ships with the games and apps that support DLSS 5, and
//! NVIDIA publishes it nowhere else. So the user's own copy is the source:
//! find the NVIDIA-signed `nvngx_dlssnr.dll` in scanned games and in NVIDIA
//! apps (RTX Remix), and copy the chosen one into our data folder. Nothing is
//! downloaded. Copies we placed ourselves are not sources; they are already
//! the user's chosen file. Only x64 builds count: Remix also carries ARM64.

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::{
    analysis::{
        dlss::{file_version, MODEL_DLL},
        pe::{self, MACHINE_AMD64},
        walk::Tree,
        WALK_DEPTH,
    },
    components::fetch::sha256_file,
    install::InstallManifest,
    library::Library,
    model_fit::{model_build, nvidia_signed, ModelBuild},
    paths::AppPaths,
    platform::{Hive, Registry, SignatureChecker},
    Error, Result,
};

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

/// NVIDIA apps that carry the model: uninstall `DisplayName` prefix, and where
/// the x64 model sits under `InstallLocation`.
const NVIDIA_APPS: &[(&str, &str, &[&str])] = &[(
    "RTX Remix",
    "NVIDIA RTX Remix",
    &[
        r"deps\remix_runtime\runtime\.trex\nvngx_dlssnr.dll",
        r"exts\lightspeed.hydra.remix.core\deps\hdremix\nvngx_dlssnr.dll",
    ],
)];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ModelSource {
    /// The game or app it came from.
    pub title: String,
    pub path: String,
    #[ts(type = "number")]
    pub size: u64,
    pub sha256: String,
    /// File version, e.g. `310.9.16.0`.
    pub version: Option<String>,
    pub build: ModelBuild,
}

/// NVIDIA-signed x64 models in the library's games and in NVIDIA apps, one per
/// distinct file, newest version first.
pub fn find(
    library: &Library,
    paths: &AppPaths,
    registry: &dyn Registry,
    sig: &dyn SignatureChecker,
) -> Vec<ModelSource> {
    let ours = placed_by_us(library, paths);
    let mut candidates: Vec<(String, PathBuf)> = nvidia_apps(registry);
    for game in &library.games {
        if !game.analysis.as_ref().is_some_and(|a| a.has_dlss5_model) {
            continue;
        }
        let tree = Tree::collect(&game.install_dir, WALK_DEPTH);
        candidates.extend(
            tree.files_where(|f| f.name == MODEL_DLL)
                .map(|f| (game.title.clone(), f.path.clone())),
        );
    }

    let mut out: Vec<ModelSource> = Vec::new();
    for (title, path) in candidates {
        if ours.contains(&path) || !is_x64(&path) {
            continue;
        }
        let (Ok(meta), Ok(sha256)) = (std::fs::metadata(&path), sha256_file(&path)) else {
            continue;
        };
        if out.iter().any(|s| s.sha256 == sha256) {
            continue;
        }
        let signature = sig.check(&path);
        if !nvidia_signed(&signature) {
            continue;
        }
        out.push(ModelSource {
            title,
            path: path.display().to_string(),
            size: meta.len(),
            version: file_version(&path),
            build: model_build(&sha256, &signature),
            sha256,
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(version_key(s.version.as_deref())));
    out
}

/// Model files of installed NVIDIA apps, from their uninstall entries.
fn nvidia_apps(registry: &dyn Registry) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for sub in registry.subkeys(Hive::LocalMachine, UNINSTALL) {
        let key = format!(r"{UNINSTALL}\{sub}");
        let Some(name) = registry.read_string(Hive::LocalMachine, &key, "DisplayName") else {
            continue;
        };
        let Some(dir) = registry.read_string(Hive::LocalMachine, &key, "InstallLocation") else {
            continue;
        };
        for (title, prefix, rels) in NVIDIA_APPS {
            if name.starts_with(prefix) {
                // One component at a time, so the core's tests run off Windows.
                let join = |rel: &str| rel.split('\\').fold(PathBuf::from(&dir), |p, c| p.join(c));
                out.extend(rels.iter().map(|rel| (title.to_string(), join(rel))));
            }
        }
    }
    out
}

fn is_x64(path: &Path) -> bool {
    pe::read_header(path).is_ok_and(|info| info.machine == MACHINE_AMD64)
}

fn version_key(version: Option<&str>) -> Vec<u32> {
    version
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
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
    if !is_x64(src) {
        return Err(Error::UserFileMissing {
            what: "DLSS 5 model".into(),
            detail: format!("{} is not an x64 build", src.display()),
        });
    }
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
    use crate::analysis::pe::testing::{build_pe, X64};
    use crate::install::manifest::{FileRecord, INSTALL_SCHEMA};
    use crate::install::planner::testing::game;
    use crate::model::{GraphicsApi, Route};
    use crate::platform::{testing::FakeRegistry, Signature};

    const ARM64: u16 = 0xAA64;

    /// Files containing "nv-signed" are NVIDIA-signed; others were modified.
    struct FakeSig;
    impl SignatureChecker for FakeSig {
        fn check(&self, file: &Path) -> Signature {
            match std::fs::read(file) {
                Ok(b) if b.windows(9).any(|w| w == b"nv-signed") => Signature::Valid {
                    signer: "NVIDIA Corporation".into(),
                },
                Ok(_) => Signature::HashMismatch {
                    signer: "NVIDIA Corporation".into(),
                },
                Err(e) => Signature::Unknown(e.to_string()),
            }
        }
    }

    /// A model DLL for `machine` whose bytes carry `marker`.
    fn model(path: &Path, machine: u16, marker: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, build_pe(machine, &[(".rdata", marker.as_bytes())])).unwrap();
    }

    fn with_model(dir: &Path, id: &str, title: &str) -> crate::model::Game {
        let mut g = game(dir, &[GraphicsApi::Dx12], Route::ReShadeRenoDx, true);
        g.id = id.into();
        g.title = title.into();
        g.analysis.as_mut().unwrap().has_dlss5_model = true;
        g
    }

    #[test]
    fn lists_signed_x64_models_once_and_skips_modified_arm_and_our_own_copies() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        paths.ensure().unwrap();
        let root = tmp.path();
        model(
            &root.join("Ball/Bin/nvngx_dlssnr.dll"),
            X64,
            "nv-signed october",
        );
        model(
            &root.join("Twin/nvngx_dlssnr.dll"),
            X64,
            "nv-signed october",
        );
        model(
            &root.join("Twin/arm/nvngx_dlssnr.dll"),
            ARM64,
            "nv-signed arm",
        );
        model(&root.join("Patched/nvngx_dlssnr.dll"), X64, "patched");
        model(&root.join("Ours/nvngx_dlssnr.dll"), X64, "nv-signed ours");
        std::fs::create_dir_all(root.join("NoModel")).unwrap();
        let library = Library {
            games: vec![
                with_model(&root.join("Ball"), "b", "Ball"),
                with_model(&root.join("Twin"), "t", "Twin"),
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

        let found = find(&library, &paths, &FakeRegistry::default(), &FakeSig);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].title, "Ball");
        assert_eq!(found[0].build, ModelBuild::NvidiaSigned);
    }

    #[test]
    fn finds_rtx_remix_from_its_uninstall_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        let remix = tmp.path().join("RTX Remix");
        model(
            &remix.join("deps/remix_runtime/runtime/.trex/nvngx_dlssnr.dll"),
            X64,
            "nv-signed remix",
        );
        model(
            &remix.join("deps/remix_runtime/runtime/.trex_arm64/nvngx_dlssnr.dll"),
            ARM64,
            "nv-signed arm",
        );
        let mut reg = FakeRegistry::default();
        reg.add(
            Hive::LocalMachine,
            &format!(r"{UNINSTALL}\{{B2FE}}_RTX.Remix"),
            &[
                ("DisplayName", "NVIDIA RTX Remix 1.6.1.0"),
                ("InstallLocation", &remix.display().to_string()),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            &format!(r"{UNINSTALL}\Other"),
            &[("DisplayName", "Something Else"), ("InstallLocation", "x")],
        );
        let found = find(&Library::default(), &paths, &reg, &FakeSig);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].title, "RTX Remix");
        assert!(found[0].path.contains(".trex"));
        assert!(!found[0].path.contains("arm64"));
    }

    #[test]
    fn adopt_copies_a_signed_model_and_refuses_anything_else() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(tmp.path().join("data"));
        let src = tmp.path().join("a/nvngx_dlssnr.dll");
        model(&src, X64, "nv-signed model");
        model(&tmp.path().join("b/nvngx_dlssnr.dll"), X64, "patched");
        model(
            &tmp.path().join("c/nvngx_dlssnr.dll"),
            ARM64,
            "nv-signed arm",
        );
        let copy = adopt(&src, &paths, &FakeSig).unwrap();
        assert!(copy.starts_with(paths.user_files_dir()));
        assert_eq!(std::fs::read(&copy).unwrap(), std::fs::read(&src).unwrap());
        assert_eq!(adopt(&src, &paths, &FakeSig).unwrap(), copy, "idempotent");

        for bad in ["b", "c"] {
            let err = adopt(&tmp.path().join(bad).join(MODEL_DLL), &paths, &FakeSig).unwrap_err();
            assert_eq!(err.code(), "user_file_missing", "{bad}");
        }
    }
}
