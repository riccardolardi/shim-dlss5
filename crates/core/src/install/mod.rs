//! Install planning. Phase 1 ships only the preview: the list of files a
//! route would add, back up or edit, so "What will change" is honest before
//! any install code exists. Phase 2 turns this into executable `FileOp`s.

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

use crate::{
    analysis::dlss::MODEL_DLL,
    model::{Analysis, Game, GameStatus, GraphicsApi, Route},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeKind {
    /// A new file we write; removed on uninstall.
    Add,
    /// An existing file we overwrite; backed up first, restored on uninstall.
    Backup,
    /// A text file we edit in place; backed up first.
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PlannedChange {
    pub kind: ChangeKind,
    pub path: PathBuf,
    /// Where the bytes come from or what the edit is, in one short phrase.
    pub note: String,
}

/// Proxy names OptiScaler can take, in the order we try them (PLAN §5.5).
pub const OPTISCALER_SLOTS_DX: &[&str] = &[
    "dxgi.dll",
    "winmm.dll",
    "version.dll",
    "dbghelp.dll",
    "d3d12.dll",
    "wininet.dll",
    "winhttp.dll",
];
pub const OPTISCALER_SLOTS_VK: &[&str] = &["winmm.dll", "version.dll", "dbghelp.dll"];

/// The preview for a game, or empty when it has no route.
pub fn preview(game: &Game) -> Vec<PlannedChange> {
    let (Some(analysis), GameStatus::Ready { route, .. }) = (&game.analysis, &game.status) else {
        return Vec::new();
    };
    let dir = analysis.exe.parent().unwrap_or(&game.install_dir);
    let mut out = match route {
        Route::OptiScaler => optiscaler(dir, analysis),
        Route::ReShadeRenoDx => reshade(dir, analysis, true),
        Route::ReShadeVulkan => reshade(dir, analysis, false),
    };
    out.push(add(
        dir.join("shim.json"),
        "install record (lets shim undo everything)",
    ));
    out
}

fn optiscaler(dir: &Path, a: &Analysis) -> Vec<PlannedChange> {
    let slots = if a.apis.contains(&GraphicsApi::Dx12) || a.apis.contains(&GraphicsApi::Dx11) {
        OPTISCALER_SLOTS_DX
    } else {
        OPTISCALER_SLOTS_VK
    };
    let slot = slots
        .iter()
        .find(|s| !dir.join(s).exists())
        .unwrap_or(&slots[0]);
    vec![
        write(
            dir.join(slot),
            "OptiScaler.dll, renamed to the first free proxy slot",
        ),
        write(dir.join("OptiScaler.ini"), "OptiScaler default settings"),
        write(dir.join(MODEL_DLL), "your DLSS 5 model"),
    ]
}

fn reshade(dir: &Path, a: &Analysis, renodx: bool) -> Vec<PlannedChange> {
    // ReShade hooks DX11 and DX12 alike through dxgi.dll; Vulkan goes through
    // its layer mechanism, so the DLL keeps its own name.
    let vulkan_only = !a.apis.contains(&GraphicsApi::Dx12) && !a.apis.contains(&GraphicsApi::Dx11);
    let mut out = Vec::new();
    if vulkan_only {
        out.push(write(
            dir.join("ReShade64.dll"),
            "ReShade add-on build (Vulkan layer)",
        ));
    } else {
        out.push(write(
            dir.join("dxgi.dll"),
            "ReShade64.dll, renamed to the proxy slot",
        ));
    }
    out.push(write(
        dir.join("ReShade.ini"),
        "minimal ReShade settings (add-ons enabled)",
    ));
    if renodx {
        out.push(write(
            dir.join("renodx-dlss5.addon64"),
            "RenoDX DLSS 5 add-on",
        ));
    }
    out.push(write(
        dir.join("dlss5-feed.addon64"),
        "DLSS 5 feeder add-on",
    ));
    out.push(write(dir.join(MODEL_DLL), "your DLSS 5 model"));
    out
}

/// Add when the target does not exist yet, otherwise Backup.
fn write(path: PathBuf, note: &str) -> PlannedChange {
    let kind = if path.exists() {
        ChangeKind::Backup
    } else {
        ChangeKind::Add
    };
    PlannedChange {
        kind,
        path,
        note: note.to_string(),
    }
}

fn add(path: PathBuf, note: &str) -> PlannedChange {
    PlannedChange {
        kind: ChangeKind::Add,
        path,
        note: note.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bitness, Engine, Launcher};

    fn game(dir: &Path, apis: &[GraphicsApi], route: Route) -> Game {
        Game {
            id: "id".into(),
            launcher: Launcher::Steam,
            title: "G".into(),
            install_dir: dir.to_path_buf(),
            launcher_id: None,
            analysis: Some(Analysis {
                exe: dir.join("G.exe"),
                exe_size: 1,
                exe_mtime: 1,
                bitness: Bitness::X64,
                apis: apis.to_vec(),
                engine: Engine::Other,
                ships_dlss: route == Route::OptiScaler,
                dlss_version: None,
                has_dlss5_model: false,
                anti_cheat: None,
                foreign_reshade: false,
                foreign_optiscaler: false,
            }),
            status: GameStatus::Ready {
                route,
                reason: String::new(),
            },
            cover: None,
            hidden: false,
        }
    }

    fn names(changes: &[PlannedChange]) -> Vec<String> {
        changes
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn optiscaler_takes_the_first_free_slot_and_backs_up_an_occupied_one() {
        let tmp = tempfile::tempdir().unwrap();
        let g = game(tmp.path(), &[GraphicsApi::Dx12], Route::OptiScaler);
        let plan = preview(&g);
        assert_eq!(
            names(&plan),
            vec![
                "dxgi.dll",
                "OptiScaler.ini",
                "nvngx_dlssnr.dll",
                "shim.json"
            ]
        );
        assert!(plan.iter().all(|c| c.kind == ChangeKind::Add));

        std::fs::write(tmp.path().join("dxgi.dll"), b"taken").unwrap();
        let plan = preview(&g);
        assert_eq!(names(&plan)[0], "winmm.dll");

        std::fs::write(tmp.path().join("OptiScaler.ini"), b"[old]").unwrap();
        let plan = preview(&g);
        assert_eq!(plan[1].kind, ChangeKind::Backup);
    }

    #[test]
    fn reshade_routes_list_their_addons() {
        let tmp = tempfile::tempdir().unwrap();
        let dx = preview(&game(
            tmp.path(),
            &[GraphicsApi::Dx11],
            Route::ReShadeRenoDx,
        ));
        assert_eq!(
            names(&dx),
            vec![
                "dxgi.dll",
                "ReShade.ini",
                "renodx-dlss5.addon64",
                "dlss5-feed.addon64",
                "nvngx_dlssnr.dll",
                "shim.json"
            ]
        );
        let vk = preview(&game(
            tmp.path(),
            &[GraphicsApi::Vulkan],
            Route::ReShadeVulkan,
        ));
        assert_eq!(names(&vk)[0], "ReShade64.dll");
        assert!(!names(&vk).iter().any(|n| n.starts_with("renodx")));
    }

    #[test]
    fn games_without_a_route_have_an_empty_preview() {
        let tmp = tempfile::tempdir().unwrap();
        let mut g = game(tmp.path(), &[GraphicsApi::Dx9], Route::OptiScaler);
        g.status = GameStatus::Unsupported { reason: "x".into() };
        assert!(preview(&g).is_empty());
        g.status = GameStatus::Pending;
        assert!(preview(&g).is_empty());
    }
}
