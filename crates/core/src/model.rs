//! Domain types shared between the core, the Tauri layer and the front end.
//!
//! Every public type here is exported to TypeScript by `ts-rs` when the core
//! tests run (`cargo test -p shim-core`). The generated files land in
//! `src/lib/generated/` and are committed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Launcher {
    Steam,
    Epic,
    Gog,
    Xbox,
    Ubisoft,
    Ea,
    Custom,
}

impl Launcher {
    pub fn label(self) -> &'static str {
        match self {
            Self::Steam => "Steam",
            Self::Epic => "Epic Games",
            Self::Gog => "GOG",
            Self::Xbox => "Xbox",
            Self::Ubisoft => "Ubisoft Connect",
            Self::Ea => "EA app",
            Self::Custom => "Custom folder",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GraphicsApi {
    Dx9,
    Dx10,
    Dx11,
    Dx12,
    Vulkan,
    OpenGl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Bitness {
    X86,
    X64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AntiCheat {
    EasyAntiCheat,
    BattlEye,
    Vanguard,
    Ricochet,
    GameGuard,
    Xigncode,
    Javelin,
    Ace,
    Mhyprot,
    Vac,
    Other,
}

/// Informational engine guess from the folder layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Engine {
    Unreal,
    Unity,
    RedEngine,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Route {
    /// Game ships DLSS: ReShade + RenoDX DLSS 5 add-on + model. The add-on
    /// hooks the game's own NGX calls, so DLSS must be on in the game.
    #[serde(rename = "reshade_renodx")]
    ReShadeRenoDx,
    /// No DLSS, DX11/DX12: ReShade + Feeder + RenoDX add-on + DLSS runtime + model.
    #[serde(rename = "reshade_feeder")]
    ReShadeFeeder,
    /// Game ships DLSS, user chose [`InstallMode::OptiScalerDlss5`]: the
    /// OptiScaler DLSS-NR fork as the proxy, model beside it, NR switched on.
    #[serde(rename = "optiscaler_dlssnr")]
    OptiScalerDlssNr,
    /// Game ships DLSS, user chose [`InstallMode::OptiScalerOnly`]: upstream
    /// OptiScaler, no model. (Also what shim 0.1.0 installed by default.)
    #[serde(rename = "optiscaler")]
    OptiScaler,
    /// No DLSS, Vulkan: not routed yet (Phase 3 note in PLAN §12.0c).
    #[serde(rename = "reshade_vulkan")]
    ReShadeVulkan,
}

/// The user's per-game choice for games that ship DLSS. Games without DLSS
/// always take the Feeder route; the choice is ignored there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum InstallMode {
    /// ReShade + RenoDX DLSS 5 add-on over the game's own DLSS output.
    #[default]
    Dlss5,
    /// OptiScaler's upscaler plus the DLSS 5 model in one DLL (the DLSS-NR fork).
    OptiScalerDlss5,
    /// Upstream OptiScaler only: a different upscaler, no neural pass.
    OptiScalerOnly,
}

/// What the Library card says under the title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum GameStatus {
    /// Scanned, nothing installed, a route exists. `reason` is the one
    /// sentence the Game screen shows for why this route was chosen.
    Ready {
        route: Route,
        reason: String,
    },
    Installed {
        route: Route,
    },
    UpdateAvailable {
        route: Route,
    },
    AntiCheat {
        which: AntiCheat,
    },
    Unsupported {
        reason: String,
    },
    /// Discovered but not analysed yet.
    Pending,
}

/// What a launcher adapter hands back before analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiscoveredGame {
    pub launcher: Launcher,
    pub title: String,
    pub install_dir: PathBuf,
    /// Executable the launcher declares, if any. Verified later by analysis.
    pub declared_exe: Option<PathBuf>,
    /// Launcher-native id, e.g. the Steam app id. Informational.
    pub launcher_id: Option<String>,
}

/// Facts read from the game folder and executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Analysis {
    pub exe: PathBuf,
    /// Size and mtime of `exe` when analysed; a changed pair invalidates the cache.
    #[ts(type = "number")]
    pub exe_size: u64,
    #[ts(type = "number")]
    pub exe_mtime: u64,
    pub bitness: Bitness,
    pub apis: Vec<GraphicsApi>,
    pub engine: Engine,
    pub ships_dlss: bool,
    pub dlss_version: Option<String>,
    pub has_dlss5_model: bool,
    pub anti_cheat: Option<AntiCheat>,
    pub foreign_reshade: bool,
    pub foreign_optiscaler: bool,
}

/// A game in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Game {
    /// `sha256(normalised install_dir)`, stable across runs.
    pub id: String,
    pub launcher: Launcher,
    pub title: String,
    pub install_dir: PathBuf,
    pub launcher_id: Option<String>,
    pub analysis: Option<Analysis>,
    pub status: GameStatus,
    pub cover: Option<PathBuf>,
    pub hidden: bool,
    /// Per-game route choice; `None` means the default ([`InstallMode::Dlss5`]).
    #[serde(default)]
    pub mode: Option<InstallMode>,
}
