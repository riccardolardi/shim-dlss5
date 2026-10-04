//! Everything we learn about a game by reading its folder. Pure functions over
//! paths; nothing here writes.

pub mod anticheat;
pub mod apis;
pub mod dlss;
pub mod exe_resolver;
pub mod pe;
pub mod walk;

use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::{
    model::{Analysis, Bitness, DiscoveredGame},
    Error, Result,
};

/// How deep to look for executables and marker files. Unreal keeps DLSS at
/// `Engine/Plugins/Marketplace/DLSS/Binaries/ThirdParty/Win64/`, nine levels down.
pub const WALK_DEPTH: usize = 9;

/// Bump whenever detection rules change, so cached analyses from older
/// builds are redone on the next scan instead of trusted.
/// 2: logs and shader packs no longer count as a foreign ReShade/OptiScaler.
pub const ANALYSIS_VERSION: u32 = 2;

/// Analyse a game folder from scratch.
pub fn analyse(game: &DiscoveredGame) -> Result<Analysis> {
    let tree = walk::Tree::collect(&game.install_dir, WALK_DEPTH);
    let exe = exe_resolver::resolve(&tree, &game.title, game.declared_exe.as_deref()).ok_or(
        Error::NoExecutable {
            install_dir: game.install_dir.clone(),
        },
    )?;
    analyse_exe(&tree, &exe, &game.title)
}

fn analyse_exe(tree: &walk::Tree, exe: &Path, title: &str) -> Result<Analysis> {
    let info = pe::read_header(exe).map_err(|e| Error::NotAnExecutable {
        path: exe.to_path_buf(),
        detail: e.to_string(),
    })?;
    let bitness = info.bitness().unwrap_or(Bitness::X86);
    let bytes = pe::read_sections_or_file(exe, &info, &[".rdata", ".idata", ".data"]);
    let exe_dir = exe.parent().unwrap_or(&tree.root);
    let facts = dlss::inspect(tree, exe_dir);
    let (exe_size, exe_mtime) = fingerprint(exe);

    Ok(Analysis {
        exe: exe.to_path_buf(),
        exe_size,
        exe_mtime,
        version: ANALYSIS_VERSION,
        bitness,
        apis: apis::detect(&bytes),
        engine: exe_resolver::engine(tree, exe),
        ships_dlss: facts.ships_dlss,
        dlss_version: facts.dlss_version,
        has_dlss5_model: facts.has_model,
        anti_cheat: anticheat::detect(tree, title),
        foreign_reshade: facts.foreign_reshade,
        foreign_optiscaler: facts.foreign_optiscaler,
    })
}

/// Reuse `previous` when its executable is unchanged, otherwise re-analyse.
pub fn analyse_cached(game: &DiscoveredGame, previous: Option<&Analysis>) -> Result<Analysis> {
    if let Some(prev) = previous {
        if prev.version == ANALYSIS_VERSION
            && prev.exe.is_file()
            && fingerprint(&prev.exe) == (prev.exe_size, prev.exe_mtime)
        {
            return Ok(prev.clone());
        }
    }
    analyse(game)
}

fn fingerprint(exe: &Path) -> (u64, u64) {
    let Ok(meta) = std::fs::metadata(exe) else {
        return (0, 0);
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (meta.len(), mtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::pe::testing::{write_exe, X64, X86};
    use crate::analysis::walk::testing::make_tree;
    use crate::model::{AntiCheat, Engine, GraphicsApi, Launcher};
    use std::path::PathBuf;

    fn game(dir: &Path, title: &str) -> DiscoveredGame {
        DiscoveredGame {
            launcher: Launcher::Steam,
            title: title.into(),
            install_dir: dir.to_path_buf(),
            declared_exe: None,
            launcher_id: None,
        }
    }

    #[test]
    fn full_analysis_of_an_unreal_dx12_game_with_dlss_and_eac() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_exe(&root.join("Game.exe"), X64, &["launcher"]);
        write_exe(
            &root.join("Game/Binaries/Win64/Game-Win64-Shipping.exe"),
            X64,
            &["d3d12.dll", "D3D11CreateDevice", "vulkan-1.dll"],
        );
        make_tree(
            root,
            &[
                (
                    "Engine/Binaries/ThirdParty/NVIDIA/NGX/Win64/nvngx_dlss.dll",
                    b"",
                ),
                ("EasyAntiCheat/EasyAntiCheat_EOS_Setup.exe", b""),
            ],
        );
        let a = analyse(&game(root, "Game")).unwrap();
        assert!(a.exe.ends_with("Game-Win64-Shipping.exe"));
        assert_eq!(a.bitness, Bitness::X64);
        assert_eq!(
            a.apis,
            vec![GraphicsApi::Dx12, GraphicsApi::Vulkan, GraphicsApi::Dx11]
        );
        assert_eq!(a.engine, Engine::Unreal);
        assert!(a.ships_dlss);
        assert_eq!(a.dlss_version, None, "empty dll has no version resource");
        assert_eq!(a.anti_cheat, Some(AntiCheat::EasyAntiCheat));
        assert!(!a.foreign_reshade);
        assert!(a.exe_size > 0);
    }

    #[test]
    fn thirty_two_bit_game_is_reported_as_x86() {
        let tmp = tempfile::tempdir().unwrap();
        write_exe(&tmp.path().join("old.exe"), X86, &["d3d9.dll"]);
        let a = analyse(&game(tmp.path(), "Old")).unwrap();
        assert_eq!(a.bitness, Bitness::X86);
        assert_eq!(a.apis, vec![GraphicsApi::Dx9]);
    }

    #[test]
    fn folder_without_exe_is_a_typed_error() {
        let tmp = tempfile::tempdir().unwrap();
        make_tree(tmp.path(), &[("readme.txt", b"")]);
        let err = analyse(&game(tmp.path(), "Nothing")).unwrap_err();
        assert_eq!(err.code(), "no_executable");
        assert!(err.user_message().contains("executable"));
    }

    #[test]
    fn non_pe_exe_is_a_typed_error() {
        let tmp = tempfile::tempdir().unwrap();
        make_tree(tmp.path(), &[("fake.exe", b"#!/bin/sh")]);
        let err = analyse(&game(tmp.path(), "Fake")).unwrap_err();
        assert_eq!(err.code(), "not_an_executable");
    }

    #[test]
    fn cache_is_reused_until_the_exe_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("g.exe");
        write_exe(&exe, X64, &["d3d11.dll"]);
        let g = game(tmp.path(), "g");
        let first = analyse(&g).unwrap();
        let mut stale = first.clone();
        stale.apis = vec![GraphicsApi::OpenGl];
        assert_eq!(analyse_cached(&g, Some(&stale)).unwrap().apis, stale.apis);

        // Sections are 512-byte aligned, so the padding must cross that to change the size.
        let padding = "x".repeat(2000);
        write_exe(&exe, X64, &["d3d12.dll", &padding]);
        let fresh = analyse_cached(&g, Some(&stale)).unwrap();
        assert_eq!(fresh.apis, vec![GraphicsApi::Dx12]);

        let gone = Analysis {
            exe: PathBuf::from("Z:/nope.exe"),
            ..stale.clone()
        };
        assert_eq!(analyse_cached(&g, Some(&gone)).unwrap().apis, fresh.apis);

        // A cache written by an older build's rules is not trusted either.
        let old_rules = Analysis {
            version: ANALYSIS_VERSION - 1,
            ..fresh.clone()
        };
        let mut poisoned = old_rules.clone();
        poisoned.apis = vec![GraphicsApi::OpenGl];
        assert_eq!(
            analyse_cached(&g, Some(&poisoned)).unwrap().apis,
            fresh.apis
        );
    }
}
