//! Installing into a game folder: plan → journal → manifest, and the exact
//! reverse for uninstall. Every byte written is recorded; every byte
//! overwritten is backed up first.

pub mod journal;
pub mod manifest;
pub mod planner;

use std::path::PathBuf;

use serde::Serialize;
use ts_rs::TS;

pub use journal::{recover, recover_all, rollback, uninstall, Journal, Step};
pub use manifest::InstallManifest;
pub use planner::{plan, FileOp, Inputs, Plan};

use crate::model::{Game, GameStatus, Route};

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

/// Why a real plan could not be built, for the Game page to show beside the
/// sketch so the user knows what to fetch or choose first.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Preview {
    pub changes: Vec<PlannedChange>,
    /// True when `changes` comes from a buildable plan; false for the sketch.
    pub exact: bool,
    pub blocker: Option<crate::error::ErrorDto>,
}

/// The exact plan when everything is in place, otherwise a sketch of the
/// route plus the reason the exact plan is not available yet.
pub fn preview(inputs: &Inputs<'_>) -> Preview {
    match plan(inputs) {
        Ok(p) => Preview {
            changes: p.describe(),
            exact: true,
            blocker: None,
        },
        Err(e) => Preview {
            changes: sketch(inputs.game),
            exact: false,
            blocker: Some(e.to_dto()),
        },
    }
}

/// The route's file list from names alone, for games whose components or
/// user files are not ready yet. Empty when the game has no route.
pub fn sketch(game: &Game) -> Vec<PlannedChange> {
    let (Some(analysis), GameStatus::Ready { route, .. }) = (&game.analysis, &game.status) else {
        return Vec::new();
    };
    let dir = analysis.exe.parent().unwrap_or(&game.install_dir);
    let add = |name: &str, note: &str| PlannedChange {
        kind: if dir.join(name).exists() {
            ChangeKind::Backup
        } else {
            ChangeKind::Add
        },
        path: dir.join(name),
        note: note.to_string(),
    };
    let mut out = Vec::new();
    match route {
        Route::ReShadeRenoDx | Route::ReShadeFeeder => {
            out.push(add("dxgi.dll", "ReShade64.dll, renamed to the proxy slot"));
            out.push(add(
                "ReShade.ini",
                "ReShade settings: add-ons on, DLSS 5 add-on configured",
            ));
            if *route == Route::ReShadeFeeder {
                out.push(add(
                    "ReShadePreset.ini",
                    "preset that turns the DLSS 5 Feed effect on",
                ));
                out.push(add("dlss5-feed.addon64", "DLSS 5 feeder add-on"));
                out.push(add(
                    "reshade-shaders\\Shaders\\DLSS5_Feed.fx",
                    "feeder effect",
                ));
                out.push(add("nvngx_dlss.dll", "your DLSS runtime"));
            }
            out.push(add("renodx-dlss5.addon64", "your RenoDX DLSS 5 add-on"));
            out.push(add("nvngx_dlssnr.dll", "your DLSS 5 model"));
        }
        Route::OptiScalerDlssNr => {
            out.push(add(
                "dxgi.dll",
                "OptiScaler DLSS-NR fork, in the first free proxy slot",
            ));
            out.push(add(
                "OptiScaler.ini",
                "settings with the DLSS 5 pass switched on",
            ));
            out.push(add("nvngx.dll_dlssnr.dll", "the fork's NGX snippet"));
            out.push(add(
                "OptiScaler\\libxess.dll",
                "OptiScaler payload (several DLLs)",
            ));
            out.push(add("nvngx_dlssnr.dll", "your DLSS 5 model"));
        }
        Route::OptiScaler => {
            out.push(add("dxgi.dll", "OptiScaler, in the first free proxy slot"));
            out.push(add("OptiScaler.ini", "OptiScaler default settings"));
            out.push(add("libxess.dll", "OptiScaler payload (several DLLs)"));
        }
        Route::ReShadeVulkan => return Vec::new(),
    }
    out.push(add(
        "shim.json",
        "install record (lets shim undo everything)",
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::ComponentStore;
    use crate::install::planner::testing::*;
    use crate::model::GraphicsApi;
    use crate::settings::Settings;

    #[test]
    fn preview_is_exact_when_ready_and_a_sketch_with_blocker_otherwise() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, components) = fake_store(&tmp.path().join("store"));
        let game_dir = tmp.path().join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let game = game(&game_dir, &[GraphicsApi::Dx12], Route::ReShadeRenoDx, true);

        let settings = settings_with_user_files(tmp.path());
        let exact = preview(&Inputs {
            game: &game,
            store: &store,
            components: &components,
            settings: &settings,
        });
        assert!(exact.exact && exact.blocker.is_none());
        assert!(exact
            .changes
            .iter()
            .any(|c| c.path.ends_with("renodx-dlss5.addon64")));

        let none = Settings::default();
        let empty = ComponentStore::at(tmp.path().join("empty"));
        let sketched = preview(&Inputs {
            game: &game,
            store: &empty,
            components: &components,
            settings: &none,
        });
        assert!(!sketched.exact);
        assert_eq!(sketched.blocker.unwrap().code, "component_missing");
        assert!(sketched
            .changes
            .iter()
            .any(|c| c.path.ends_with("shim.json")));

        let mut unsupported = game.clone();
        unsupported.status = GameStatus::Unsupported { reason: "x".into() };
        assert!(sketch(&unsupported).is_empty());
    }
}
