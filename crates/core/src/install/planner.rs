//! Turn a game, its route and the component store into the exact list of file
//! operations an install performs. The plan is a value: nothing here touches
//! the game folder, and `describe()` is what the user reads before clicking.

use std::path::{Path, PathBuf};

use crate::{
    analysis::dlss::MODEL_DLL,
    components::{ComponentManifest, ComponentStore},
    install::{manifest::ComponentPin, ChangeKind, PlannedChange},
    model::{Game, GameStatus, GraphicsApi, Route},
    settings::Settings,
    Error, Result,
};

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

/// Store folders of OptiScaler that never go beside a game exe.
const OPTISCALER_SKIP: &[&str] = &["Licenses/", "D3D12_Optiscaler/"];

pub const RESHADE_PROXY: &str = "dxgi.dll";
pub const RESHADE_INI: &str = "ReShade.ini";
pub const RESHADE_PRESET: &str = "ReShadePreset.ini";
pub const RENODX_ADDON: &str = "renodx-dlss5.addon64";
pub const DLSS_RUNTIME_DLL: &str = "nvngx_dlss.dll";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOp {
    /// Copy a verified file from the store or the user's own file.
    Copy {
        src: PathBuf,
        dst: PathBuf,
        note: String,
    },
    /// Write a small text file we generate (ini / preset).
    WriteText {
        dst: PathBuf,
        text: String,
        note: String,
    },
}

impl FileOp {
    pub fn dst(&self) -> &Path {
        match self {
            Self::Copy { dst, .. } | Self::WriteText { dst, .. } => dst,
        }
    }

    pub fn note(&self) -> &str {
        match self {
            Self::Copy { note, .. } | Self::WriteText { note, .. } => note,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub route: Route,
    pub exe: PathBuf,
    pub ops: Vec<FileOp>,
    pub components: Vec<ComponentPin>,
    pub model_sha256: Option<String>,
    pub user_files: Vec<(String, String)>,
}

impl Plan {
    /// What the user reads under "What will change".
    pub fn describe(&self) -> Vec<PlannedChange> {
        let mut out: Vec<PlannedChange> = self
            .ops
            .iter()
            .map(|op| PlannedChange {
                kind: if op.dst().exists() {
                    ChangeKind::Backup
                } else {
                    ChangeKind::Add
                },
                path: op.dst().to_path_buf(),
                note: op.note().to_string(),
            })
            .collect();
        out.push(PlannedChange {
            kind: ChangeKind::Add,
            path: self.exe_dir().join(crate::install::manifest::SIDECAR),
            note: "install record (lets shim undo everything)".into(),
        });
        out
    }

    pub fn exe_dir(&self) -> &Path {
        self.exe.parent().unwrap_or(Path::new(""))
    }
}

pub struct Inputs<'a> {
    pub game: &'a Game,
    pub store: &'a ComponentStore,
    pub components: &'a ComponentManifest,
    pub settings: &'a Settings,
}

/// Build the plan. Fails (typed) when a component, a user file or a proxy
/// slot is not available, so the UI can say exactly what to fix.
pub fn plan(inputs: &Inputs<'_>) -> Result<Plan> {
    let game = inputs.game;
    let (Some(analysis), route) = (&game.analysis, route_of(&game.status)?) else {
        return Err(Error::NoExecutable {
            install_dir: game.install_dir.clone(),
        });
    };
    let exe_dir = analysis
        .exe
        .parent()
        .unwrap_or(&game.install_dir)
        .to_path_buf();
    let model = user_file(
        inputs.settings.model_path.as_deref(),
        "DLSS 5 model (nvngx_dlssnr.dll)",
    )?;

    let mut plan = Plan {
        route,
        exe: analysis.exe.clone(),
        ops: Vec::new(),
        components: Vec::new(),
        model_sha256: None,
        user_files: Vec::new(),
    };

    match route {
        Route::OptiScaler => plan_optiscaler(inputs, &exe_dir, &analysis.apis, &mut plan)?,
        Route::ReShadeRenoDx => plan_reshade_renodx(inputs, &exe_dir, &mut plan)?,
        Route::ReShadeVulkan => {
            return Err(Error::unsupported("Vulkan games without DLSS"));
        }
    }

    plan.model_sha256 = Some(crate::components::fetch::sha256_file(&model)?);
    plan.ops.push(FileOp::Copy {
        src: model,
        dst: exe_dir.join(MODEL_DLL),
        note: "your DLSS 5 model".into(),
    });
    Ok(plan)
}

fn route_of(status: &GameStatus) -> Result<Route> {
    match status {
        GameStatus::Ready { route, .. }
        | GameStatus::Installed { route }
        | GameStatus::UpdateAvailable { route } => Ok(*route),
        GameStatus::AntiCheat { which } => Err(Error::AntiCheatBlocked {
            which: format!("{which:?}"),
        }),
        GameStatus::Unsupported { reason } => Err(Error::unsupported(reason.clone())),
        GameStatus::Pending => Err(Error::unsupported("an unscanned game")),
    }
}

fn plan_optiscaler(
    inputs: &Inputs<'_>,
    exe_dir: &Path,
    apis: &[GraphicsApi],
    plan: &mut Plan,
) -> Result<()> {
    let c = component(inputs, "optiscaler")?;
    let slots = if apis.contains(&GraphicsApi::Dx12) || apis.contains(&GraphicsApi::Dx11) {
        OPTISCALER_SLOTS_DX
    } else {
        OPTISCALER_SLOTS_VK
    };
    let slot = free_slot(exe_dir, slots)?;
    for rel in inputs.store.files(c)? {
        if OPTISCALER_SKIP.iter().any(|s| rel.starts_with(s)) {
            continue;
        }
        let src = inputs.store.file(c, &rel)?;
        let (dst, note) = if rel == "OptiScaler.dll" {
            (
                exe_dir.join(slot),
                format!("OptiScaler.dll as {slot} (first free proxy slot)"),
            )
        } else {
            (
                exe_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
                format!("OptiScaler {}: {rel}", c.version),
            )
        };
        if dst.exists() && rel != "OptiScaler.ini" {
            // OptiScaler's own files beside a game mean someone else put them there.
            return Err(Error::ForeignProxyPresent { path: dst });
        }
        plan.ops.push(FileOp::Copy { src, dst, note });
    }
    plan.components.push(pin(c));
    Ok(())
}

fn plan_reshade_renodx(inputs: &Inputs<'_>, exe_dir: &Path, plan: &mut Plan) -> Result<()> {
    let reshade = component(inputs, "reshade")?;
    let feeder = component(inputs, "dlss5-feeder")?;
    let renodx = user_file(
        inputs.settings.renodx_addon_path.as_deref(),
        "RenoDX DLSS 5 add-on (renodx-dlss5.addon64)",
    )?;
    let runtime = user_file(
        inputs.settings.dlss_runtime_path.as_deref(),
        "DLSS runtime (nvngx_dlss.dll)",
    )?;

    for name in [RESHADE_PROXY, RESHADE_INI, RESHADE_PRESET, RENODX_ADDON] {
        let path = exe_dir.join(name);
        if path.exists() {
            return Err(Error::ForeignProxyPresent { path });
        }
    }

    plan.ops.push(FileOp::Copy {
        src: inputs.store.file(reshade, "ReShade64.dll")?,
        dst: exe_dir.join(RESHADE_PROXY),
        note: format!(
            "ReShade {} add-on build, as {RESHADE_PROXY}",
            reshade.version
        ),
    });
    plan.ops.push(FileOp::WriteText {
        dst: exe_dir.join(RESHADE_INI),
        text: reshade_ini(),
        note: "minimal ReShade settings: add-ons on, shaders folder, preset".into(),
    });
    plan.ops.push(FileOp::WriteText {
        dst: exe_dir.join(RESHADE_PRESET),
        text: reshade_preset(),
        note: "preset that turns the DLSS 5 Feed effect on".into(),
    });
    for rel in inputs.store.files(feeder)? {
        if rel.ends_with(".txt") {
            continue;
        }
        plan.ops.push(FileOp::Copy {
            src: inputs.store.file(feeder, &rel)?,
            dst: exe_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
            note: format!("DLSS5-Feeder {}: {rel}", feeder.version),
        });
    }
    plan.user_files.push((
        RENODX_ADDON.into(),
        crate::components::fetch::sha256_file(&renodx)?,
    ));
    plan.ops.push(FileOp::Copy {
        src: renodx,
        dst: exe_dir.join(RENODX_ADDON),
        note: "your RenoDX DLSS 5 add-on".into(),
    });
    plan.user_files.push((
        DLSS_RUNTIME_DLL.into(),
        crate::components::fetch::sha256_file(&runtime)?,
    ));
    plan.ops.push(FileOp::Copy {
        src: runtime,
        dst: exe_dir.join(DLSS_RUNTIME_DLL),
        note: "your DLSS runtime (the game ships none)".into(),
    });
    plan.components.push(pin(reshade));
    plan.components.push(pin(feeder));
    Ok(())
}

fn reshade_ini() -> String {
    [
        "[GENERAL]",
        "EffectSearchPaths=.\\reshade-shaders\\Shaders\\**",
        "TextureSearchPaths=.\\reshade-shaders\\Textures\\**",
        "PresetPath=.\\ReShadePreset.ini",
        "",
        "[ADDON]",
        "AddonPath=.\\",
        "",
        "[OVERLAY]",
        "TutorialProgress=4",
        "",
    ]
    .join("\r\n")
}

fn reshade_preset() -> String {
    [
        "PreprocessorDefinitions=DLSS5_MV_PROVIDER=0",
        "Techniques=DLSS5_Feed@DLSS5_Feed.fx",
        "TechniqueSorting=DLSS5_Feed@DLSS5_Feed.fx",
        "",
    ]
    .join("\r\n")
}

fn component<'a>(inputs: &Inputs<'a>, id: &str) -> Result<&'a crate::components::Component> {
    let c = inputs
        .components
        .get(id)
        .ok_or_else(|| Error::ComponentMissing {
            id: id.into(),
            detail: "not in the component list".into(),
        })?;
    // Fails with ComponentMissing unless the store has it verified.
    inputs.store.files(c)?;
    match inputs.store.status(c) {
        crate::components::ComponentStatus::Verified => Ok(c),
        other => Err(Error::ComponentMissing {
            id: id.into(),
            detail: format!("{other:?}"),
        }),
    }
}

fn pin(c: &crate::components::Component) -> ComponentPin {
    ComponentPin {
        id: c.id.clone(),
        version: c.version.clone(),
        sha256: c.sha256.to_lowercase(),
    }
}

fn user_file(path: Option<&Path>, what: &str) -> Result<PathBuf> {
    match path {
        Some(p) if p.is_file() => Ok(p.to_path_buf()),
        Some(p) => Err(Error::UserFileMissing {
            what: what.into(),
            detail: format!("{} is not a file", p.display()),
        }),
        None => Err(Error::UserFileMissing {
            what: what.into(),
            detail: "no file chosen".into(),
        }),
    }
}

fn free_slot(exe_dir: &Path, slots: &[&'static str]) -> Result<&'static str> {
    slots
        .iter()
        .copied()
        .find(|s| !exe_dir.join(s).exists())
        .ok_or_else(|| Error::ForeignProxyPresent {
            path: exe_dir.join(slots[0]),
        })
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use crate::components::store::testing::fake_component;
    use crate::model::{Analysis, Bitness, Engine, Launcher};

    /// A store holding fake copies of all three components.
    pub fn fake_store(root: &Path) -> (ComponentStore, ComponentManifest) {
        let store = ComponentStore::at(root);
        let optiscaler = fake_component(
            &store,
            "optiscaler",
            &[
                ("OptiScaler.dll", b"opti"),
                ("OptiScaler.ini", b"[Upscalers]\r\n"),
                ("libxess.dll", b"xess"),
                ("Licenses/XeSS_LICENSE.txt", b"lic"),
                ("D3D12_Optiscaler/D3D12Core.dll", b"core"),
            ],
        );
        let reshade = fake_component(
            &store,
            "reshade",
            &[("ReShade64.dll", b"reshade"), ("ReShade64.json", b"{}")],
        );
        let feeder = fake_component(
            &store,
            "dlss5-feeder",
            &[
                ("dlss5-feed.addon64", b"feed"),
                ("reshade-shaders/Shaders/DLSS5_Feed.fx", b"fx"),
                ("READ-ME-FIRST.txt", b"readme"),
            ],
        );
        let manifest = ComponentManifest {
            schema: 1,
            updated: "test".into(),
            components: vec![optiscaler, reshade, feeder],
        };
        (store, manifest)
    }

    pub fn game(dir: &Path, apis: &[GraphicsApi], route: Route, ships_dlss: bool) -> Game {
        Game {
            id: "g1".into(),
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
                ships_dlss,
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

    /// Settings pointing at fake user files created under `dir`.
    pub fn settings_with_user_files(dir: &Path) -> Settings {
        let model = dir.join("user").join(MODEL_DLL);
        let renodx = dir.join("user").join(RENODX_ADDON);
        let runtime = dir.join("user").join(DLSS_RUNTIME_DLL);
        std::fs::create_dir_all(dir.join("user")).unwrap();
        std::fs::write(&model, b"model").unwrap();
        std::fs::write(&renodx, b"renodx").unwrap();
        std::fs::write(&runtime, b"runtime").unwrap();
        Settings {
            model_path: Some(model),
            renodx_addon_path: Some(renodx),
            dlss_runtime_path: Some(runtime),
            ..Settings::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    fn names(plan: &Plan) -> Vec<String> {
        plan.ops
            .iter()
            .map(|op| {
                op.dst()
                    .strip_prefix(plan.exe_dir())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn optiscaler_plan_copies_payload_into_the_first_free_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, components) = fake_store(&tmp.path().join("store"));
        let game_dir = tmp.path().join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let settings = settings_with_user_files(tmp.path());
        let game = game(&game_dir, &[GraphicsApi::Dx12], Route::OptiScaler, true);
        let inputs = Inputs {
            game: &game,
            store: &store,
            components: &components,
            settings: &settings,
        };
        let plan = plan(&inputs).unwrap();
        assert_eq!(
            names(&plan),
            vec!["dxgi.dll", "OptiScaler.ini", "libxess.dll", MODEL_DLL]
        );
        assert_eq!(plan.components.len(), 1);
        assert!(plan.model_sha256.is_some());
        let described = plan.describe();
        assert_eq!(described.len(), 5);
        assert!(described.iter().all(|c| c.kind == ChangeKind::Add));

        std::fs::write(game_dir.join("dxgi.dll"), b"taken").unwrap();
        let plan = super::plan(&inputs).unwrap();
        assert!(names(&plan).contains(&"winmm.dll".to_string()));
    }

    #[test]
    fn reshade_plan_lists_every_piece_and_refuses_occupied_slots() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, components) = fake_store(&tmp.path().join("store"));
        let game_dir = tmp.path().join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let settings = settings_with_user_files(tmp.path());
        let game = game(&game_dir, &[GraphicsApi::Dx11], Route::ReShadeRenoDx, false);
        let inputs = Inputs {
            game: &game,
            store: &store,
            components: &components,
            settings: &settings,
        };
        let plan = plan(&inputs).unwrap();
        assert_eq!(
            names(&plan),
            vec![
                "dxgi.dll",
                "ReShade.ini",
                "ReShadePreset.ini",
                "dlss5-feed.addon64",
                "reshade-shaders/Shaders/DLSS5_Feed.fx",
                RENODX_ADDON,
                DLSS_RUNTIME_DLL,
                MODEL_DLL,
            ]
        );
        assert_eq!(plan.components.len(), 2);
        assert_eq!(plan.user_files.len(), 2);

        std::fs::write(game_dir.join("ReShade.ini"), b"x").unwrap();
        assert_eq!(
            super::plan(&inputs).unwrap_err().code(),
            "foreign_proxy_present"
        );
    }

    #[test]
    fn missing_user_file_or_component_is_typed() {
        let tmp = tempfile::tempdir().unwrap();
        let (store, components) = fake_store(&tmp.path().join("store"));
        let game_dir = tmp.path().join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let game = game(&game_dir, &[GraphicsApi::Dx12], Route::OptiScaler, true);

        let no_model = Settings::default();
        let inputs = Inputs {
            game: &game,
            store: &store,
            components: &components,
            settings: &no_model,
        };
        assert_eq!(plan(&inputs).unwrap_err().code(), "user_file_missing");

        let settings = settings_with_user_files(tmp.path());
        let empty_store = ComponentStore::at(tmp.path().join("empty"));
        let inputs = Inputs {
            game: &game,
            store: &empty_store,
            components: &components,
            settings: &settings,
        };
        assert_eq!(plan(&inputs).unwrap_err().code(), "component_missing");

        let mut blocked = game.clone();
        blocked.status = GameStatus::AntiCheat {
            which: crate::model::AntiCheat::BattlEye,
        };
        let inputs = Inputs {
            game: &blocked,
            store: &store,
            components: &components,
            settings: &settings,
        };
        assert_eq!(plan(&inputs).unwrap_err().code(), "anti_cheat_blocked");
    }
}
