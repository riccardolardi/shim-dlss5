//! Turn a game, its route and the component store into the exact list of file
//! operations an install performs. The plan is a value: nothing here touches
//! the game folder, and `describe()` is what the user reads before clicking.
//!
//! ReShade routes: ReShade (add-on build, as `dxgi.dll`) hosting the user's
//! RenoDX DLSS 5 add-on next to the user's model; the no-DLSS route adds
//! DLSS5-Feeder, its effect, a preset that switches it on, and the user's
//! DLSS runtime. The `ReShade.ini` mirrors a known-working install (§12.0d).
//!
//! OptiScaler routes: the whole payload beside the exe, `OptiScaler.dll`
//! renamed to the first free proxy slot; the DLSS-NR fork also gets the model
//! and an ini with the neural pass switched on.

use std::path::{Path, PathBuf};

use crate::{
    analysis::dlss::MODEL_DLL,
    components::{ComponentManifest, ComponentStore},
    install::{manifest::ComponentPin, ChangeKind, PlannedChange},
    model::{Game, GameStatus, GraphicsApi, Route},
    settings::Settings,
    Error, Result,
};

pub const RESHADE_PROXY: &str = "dxgi.dll";
pub const RESHADE_INI: &str = "ReShade.ini";
pub const RESHADE_PRESET: &str = "ReShadePreset.ini";
pub const RENODX_ADDON: &str = "renodx-dlss5.addon64";
pub const DLSS_RUNTIME_DLL: &str = "nvngx_dlss.dll";

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
const OPTISCALER_SKIP: &[&str] = &[
    "Licenses/",
    "D3D12_Optiscaler/",
    "OptiScaler/D3D12_OptiScaler/",
];

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
    let mut plan = Plan {
        route,
        exe: analysis.exe.clone(),
        ops: Vec::new(),
        components: Vec::new(),
        model_sha256: None,
        user_files: Vec::new(),
    };

    match route {
        Route::ReShadeRenoDx => {
            plan_reshade(inputs, &exe_dir, false, &mut plan)?;
            push_renodx(inputs, &exe_dir, &mut plan)?;
            push_model(inputs, &exe_dir, &mut plan)?;
        }
        Route::ReShadeFeeder => {
            plan_reshade(inputs, &exe_dir, true, &mut plan)?;
            push_renodx(inputs, &exe_dir, &mut plan)?;
            push_model(inputs, &exe_dir, &mut plan)?;
        }
        Route::OptiScalerDlssNr => {
            plan_optiscaler(
                inputs,
                "optiscaler-nr",
                &exe_dir,
                &analysis.apis,
                true,
                &mut plan,
            )?;
            push_model(inputs, &exe_dir, &mut plan)?;
        }
        Route::OptiScaler => {
            plan_optiscaler(
                inputs,
                "optiscaler",
                &exe_dir,
                &analysis.apis,
                false,
                &mut plan,
            )?;
        }
        Route::ReShadeVulkan => return Err(Error::unsupported("Vulkan games")),
    }
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

fn push_model(inputs: &Inputs<'_>, exe_dir: &Path, plan: &mut Plan) -> Result<()> {
    let model = user_file(
        inputs.settings.model_path.as_deref(),
        "DLSS 5 model (nvngx_dlssnr.dll)",
    )?;
    plan.model_sha256 = Some(crate::components::fetch::sha256_file(&model)?);
    plan.ops.push(FileOp::Copy {
        src: model,
        dst: exe_dir.join(MODEL_DLL),
        note: "your DLSS 5 model".into(),
    });
    Ok(())
}

fn push_renodx(inputs: &Inputs<'_>, exe_dir: &Path, plan: &mut Plan) -> Result<()> {
    let renodx = user_file(
        inputs.settings.renodx_addon_path.as_deref(),
        "RenoDX DLSS 5 add-on (renodx-dlss5.addon64)",
    )?;
    plan.user_files.push((
        RENODX_ADDON.into(),
        crate::components::fetch::sha256_file(&renodx)?,
    ));
    plan.ops.push(FileOp::Copy {
        src: renodx,
        dst: exe_dir.join(RENODX_ADDON),
        note: "your RenoDX DLSS 5 add-on (the neural consumer)".into(),
    });
    Ok(())
}

/// ReShade as the proxy, our ini, and (for games without DLSS) the Feeder
/// add-on, its effect, a preset that enables it, and the DLSS runtime.
fn plan_reshade(
    inputs: &Inputs<'_>,
    exe_dir: &Path,
    with_feeder: bool,
    plan: &mut Plan,
) -> Result<()> {
    let reshade = component(inputs, "reshade")?;
    let mut occupied = vec![RESHADE_PROXY, RESHADE_INI, RENODX_ADDON];
    if with_feeder {
        occupied.push(RESHADE_PRESET);
    }
    for name in occupied {
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
        note: "ReShade settings: add-ons on, DLSS 5 add-on configured".into(),
    });
    plan.components.push(pin(reshade));

    if with_feeder {
        let feeder = component(inputs, "dlss5-feeder")?;
        let runtime = user_file(
            inputs.settings.dlss_runtime_path.as_deref(),
            "DLSS runtime (nvngx_dlss.dll)",
        )?;
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
            DLSS_RUNTIME_DLL.into(),
            crate::components::fetch::sha256_file(&runtime)?,
        ));
        plan.ops.push(FileOp::Copy {
            src: runtime,
            dst: exe_dir.join(DLSS_RUNTIME_DLL),
            note: "your DLSS runtime (the game ships none)".into(),
        });
        plan.components.push(pin(feeder));
    }
    Ok(())
}

/// The OptiScaler payload (upstream or the DLSS-NR fork) beside the exe,
/// `OptiScaler.dll` in the first free proxy slot, and `OptiScaler.ini` as
/// shipped or with `[DlssNr] Enabled=true` for the fork.
fn plan_optiscaler(
    inputs: &Inputs<'_>,
    id: &str,
    exe_dir: &Path,
    apis: &[GraphicsApi],
    neural: bool,
    plan: &mut Plan,
) -> Result<()> {
    let c = component(inputs, id)?;
    let slots = if apis.contains(&GraphicsApi::Dx12) || apis.contains(&GraphicsApi::Dx11) {
        OPTISCALER_SLOTS_DX
    } else {
        OPTISCALER_SLOTS_VK
    };
    let slot = free_slot(exe_dir, slots)?;
    for rel in inputs.store.files(c)? {
        if OPTISCALER_SKIP.iter().any(|s| rel.starts_with(s)) || rel.ends_with(".txt") {
            continue;
        }
        let src = inputs.store.file(c, &rel)?;
        let dst = if rel == "OptiScaler.dll" {
            exe_dir.join(slot)
        } else {
            exe_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
        };
        if dst.exists() {
            // OptiScaler's own files beside a game mean someone else put them there.
            return Err(Error::ForeignProxyPresent { path: dst });
        }
        if rel == "OptiScaler.ini" {
            let text = std::fs::read_to_string(&src).map_err(|e| Error::io(&src, e))?;
            plan.ops.push(FileOp::WriteText {
                dst,
                text: if neural { enable_dlssnr(&text) } else { text },
                note: if neural {
                    format!("{} settings with the DLSS 5 pass switched on", c.name)
                } else {
                    format!("{} default settings", c.name)
                },
            });
            continue;
        }
        let note = if rel == "OptiScaler.dll" {
            format!("{} {} as {slot} (first free proxy slot)", c.name, c.version)
        } else {
            format!("{} {}: {rel}", c.name, c.version)
        };
        plan.ops.push(FileOp::Copy { src, dst, note });
    }
    plan.components.push(pin(c));
    Ok(())
}

/// Keys we set inside `[DlssNr]`. The fork ships the pass off; we switch it
/// on *before* the upscaler (`RunBeforeSR`), so the model works at the game's
/// render resolution rather than the output: the cheaper placement, and the
/// one a 12 GB card can afford (MSFS 2024 at 4K hung the GPU with the pass
/// after upscaling at full resolution). Frame capture is off.
pub const DLSSNR_KEYS: &[(&str, &str)] = &[
    ("Enabled", "true"),
    ("RunBeforeSR", "true"),
    ("AutoCapture", "false"),
];

/// Apply [`DLSSNR_KEYS`] inside the `[DlssNr]` section. Anything else in the
/// file is left byte-for-byte as shipped; missing keys are appended.
pub fn enable_dlssnr(ini: &str) -> String {
    let mut out = String::with_capacity(ini.len() + 64);
    let mut in_section = false;
    let mut done = [false; DLSSNR_KEYS.len()];
    for line in ini.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.starts_with('[') {
            in_section = trimmed.eq_ignore_ascii_case("[DlssNr]");
        }
        if in_section {
            let key = trimmed.trim_start().split('=').next().unwrap_or("");
            if let Some(i) = DLSSNR_KEYS.iter().position(|(k, _)| *k == key) {
                if !done[i] {
                    out.push_str(&format!(
                        "{}={}{}",
                        key,
                        DLSSNR_KEYS[i].1,
                        &line[trimmed.len()..]
                    ));
                    done[i] = true;
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    let missing: Vec<String> = DLSSNR_KEYS
        .iter()
        .zip(done)
        .filter(|(_, d)| !d)
        .map(|((k, v), _)| format!("{k}={v}\r\n"))
        .collect();
    if !missing.is_empty() {
        if !out.ends_with('\n') && !out.is_empty() {
            out.push_str("\r\n");
        }
        if !done[0] {
            out.push_str("[DlssNr]\r\n");
        }
        out.extend(missing);
    }
    out
}

/// Mirrors the working MSFS 2024 install on the dev PC (PLAN §12.0d):
/// add-ons loaded from the exe folder, overlay tutorial skipped, and the
/// RenoDX DLSS 5 section with the neural pass on and its upscaler off.
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
        "[RenoDX.DLSS5]",
        "EnableHooks=2",
        "NeuralUplift=1",
        "NREnableUpscaling=0",
        "NRIntensity=1",
        "NRPreset=0",
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

    pub const FORK_INI: &str = "[Upscalers]\r\nDx12Upscaler=auto\r\n\r\n[DlssNr]\r\nToggleKey=auto\r\n; comment\r\nEnabled=auto\r\nTransferStrength=auto\r\n";

    /// A store holding fake copies of all four components.
    pub fn fake_store(root: &Path) -> (ComponentStore, ComponentManifest) {
        let store = ComponentStore::at(root);
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
        let fork = fake_component(
            &store,
            "optiscaler-nr",
            &[
                ("OptiScaler.dll", b"opti-nr"),
                ("OptiScaler.ini", FORK_INI.as_bytes()),
                ("nvngx.dll_dlssnr.dll", b"snippet"),
                ("OptiScaler/libxess.dll", b"xess"),
                ("OptiScaler/D3D12_OptiScaler/D3D12Core.dll", b"core"),
                ("Licenses/RenoDX_ATTRIBUTION.txt", b"lic"),
                ("READ ME - DLSS Neural Rendering.txt", b"readme"),
            ],
        );
        let upstream = fake_component(
            &store,
            "optiscaler",
            &[
                ("OptiScaler.dll", b"opti"),
                ("OptiScaler.ini", b"[Upscalers]\r\nDx12Upscaler=auto\r\n"),
                ("libxess.dll", b"xess"),
                ("Licenses/XeSS_LICENSE.txt", b"lic"),
                ("D3D12_Optiscaler/D3D12Core.dll", b"core"),
            ],
        );
        let manifest = ComponentManifest {
            schema: 1,
            updated: "test".into(),
            components: vec![reshade, feeder, fork, upstream],
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
            mode: None,
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

    fn text_of(op: &FileOp) -> String {
        match op {
            FileOp::WriteText { text, .. } => text.clone(),
            other => panic!("{other:?}"),
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        store: ComponentStore,
        components: ComponentManifest,
        game_dir: PathBuf,
        settings: Settings,
    }

    fn fx() -> Fx {
        let tmp = tempfile::tempdir().unwrap();
        let (store, components) = fake_store(&tmp.path().join("store"));
        let game_dir = tmp.path().join("game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let settings = settings_with_user_files(tmp.path());
        Fx {
            _tmp: tmp,
            store,
            components,
            game_dir,
            settings,
        }
    }

    #[test]
    fn dlss_game_gets_reshade_renodx_and_model_only() {
        let f = fx();
        let game = game(
            &f.game_dir,
            &[GraphicsApi::Dx12],
            Route::ReShadeRenoDx,
            true,
        );
        let inputs = Inputs {
            game: &game,
            store: &f.store,
            components: &f.components,
            settings: &f.settings,
        };
        let plan = plan(&inputs).unwrap();
        assert_eq!(
            names(&plan),
            vec!["dxgi.dll", "ReShade.ini", RENODX_ADDON, MODEL_DLL]
        );
        assert_eq!(plan.components.len(), 1);
        assert_eq!(plan.user_files.len(), 1);
        assert!(plan.model_sha256.is_some());
        let ini = text_of(&plan.ops[1]);
        assert!(ini.contains("[RenoDX.DLSS5]"));
        assert!(ini.contains("NeuralUplift=1"));
        assert!(ini.contains("AddonPath=.\\"));
        let described = plan.describe();
        assert_eq!(described.len(), 5);
        assert!(described.iter().all(|c| c.kind == ChangeKind::Add));

        // The DLSS runtime is not required on this route.
        let no_runtime = Settings {
            dlss_runtime_path: None,
            ..f.settings.clone()
        };
        assert!(super::plan(&Inputs {
            settings: &no_runtime,
            ..inputs
        })
        .is_ok());
    }

    #[test]
    fn no_dlss_game_adds_feeder_preset_and_runtime() {
        let f = fx();
        let game = game(
            &f.game_dir,
            &[GraphicsApi::Dx11],
            Route::ReShadeFeeder,
            false,
        );
        let inputs = Inputs {
            game: &game,
            store: &f.store,
            components: &f.components,
            settings: &f.settings,
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
                DLSS_RUNTIME_DLL,
                RENODX_ADDON,
                MODEL_DLL,
            ]
        );
        assert_eq!(plan.components.len(), 2);
        assert_eq!(plan.user_files.len(), 2);

        std::fs::write(f.game_dir.join("ReShade.ini"), b"x").unwrap();
        assert_eq!(
            super::plan(&inputs).unwrap_err().code(),
            "foreign_proxy_present"
        );
    }

    #[test]
    fn optiscaler_fork_gets_payload_model_and_neural_pass_on() {
        let f = fx();
        let game = game(
            &f.game_dir,
            &[GraphicsApi::Dx12],
            Route::OptiScalerDlssNr,
            true,
        );
        let inputs = Inputs {
            game: &game,
            store: &f.store,
            components: &f.components,
            settings: &f.settings,
        };
        let plan = plan(&inputs).unwrap();
        assert_eq!(
            names(&plan),
            vec![
                "dxgi.dll",
                "OptiScaler.ini",
                "OptiScaler/libxess.dll",
                "nvngx.dll_dlssnr.dll",
                MODEL_DLL,
            ]
        );
        let ini = text_of(&plan.ops[1]);
        assert!(ini.contains("[DlssNr]\r\nToggleKey=auto\r\n; comment\r\nEnabled=true\r\n"));
        assert!(
            ini.contains("Dx12Upscaler=auto"),
            "other sections untouched"
        );
        assert_eq!(plan.components[0].id, "optiscaler-nr");
        assert!(plan.model_sha256.is_some());
        assert!(plan.user_files.is_empty());

        std::fs::write(f.game_dir.join("dxgi.dll"), b"taken").unwrap();
        let plan = super::plan(&inputs).unwrap();
        assert!(names(&plan).contains(&"winmm.dll".to_string()));
    }

    #[test]
    fn plain_optiscaler_has_no_model_and_default_ini() {
        let f = fx();
        let game = game(&f.game_dir, &[GraphicsApi::Dx12], Route::OptiScaler, true);
        let inputs = Inputs {
            game: &game,
            store: &f.store,
            components: &f.components,
            settings: &Settings::default(),
        };
        let plan = plan(&inputs).unwrap();
        assert_eq!(
            names(&plan),
            vec!["dxgi.dll", "OptiScaler.ini", "libxess.dll"]
        );
        assert_eq!(
            text_of(&plan.ops[1]),
            "[Upscalers]\r\nDx12Upscaler=auto\r\n"
        );
        assert!(plan.model_sha256.is_none());

        std::fs::write(f.game_dir.join("libxess.dll"), b"someone's").unwrap();
        assert_eq!(
            super::plan(&inputs).unwrap_err().code(),
            "foreign_proxy_present"
        );
    }

    #[test]
    fn enable_dlssnr_only_touches_its_keys_in_its_section() {
        assert_eq!(
            enable_dlssnr(
                "[A]\r\nEnabled=auto\r\nWorkingScale=auto\r\n[DlssNr]\r\nEnabled=auto\r\nX=1\r\nRunBeforeSR=auto\r\nAutoCapture=auto\r\n"
            ),
            "[A]\r\nEnabled=auto\r\nWorkingScale=auto\r\n[DlssNr]\r\nEnabled=true\r\nX=1\r\nRunBeforeSR=true\r\nAutoCapture=false\r\n"
        );
        // Section missing entirely: appended whole.
        assert_eq!(
            enable_dlssnr("[A]\nEnabled=auto\n"),
            "[A]\nEnabled=auto\n[DlssNr]\r\nEnabled=true\r\nRunBeforeSR=true\r\nAutoCapture=false\r\n"
        );
        // Section present but a key missing: only that key is appended.
        assert_eq!(
            enable_dlssnr("[DlssNr]\r\nEnabled=auto\r\n"),
            "[DlssNr]\r\nEnabled=true\r\nRunBeforeSR=true\r\nAutoCapture=false\r\n"
        );
    }

    #[test]
    fn missing_user_file_component_or_anti_cheat_is_typed() {
        let f = fx();
        let game = game(
            &f.game_dir,
            &[GraphicsApi::Dx12],
            Route::ReShadeRenoDx,
            true,
        );

        let no_files = Settings::default();
        let inputs = Inputs {
            game: &game,
            store: &f.store,
            components: &f.components,
            settings: &no_files,
        };
        assert_eq!(plan(&inputs).unwrap_err().code(), "user_file_missing");

        let empty_store = ComponentStore::at(f.game_dir.join("empty"));
        let inputs = Inputs {
            game: &game,
            store: &empty_store,
            components: &f.components,
            settings: &f.settings,
        };
        assert_eq!(plan(&inputs).unwrap_err().code(), "component_missing");

        let mut blocked = game.clone();
        blocked.status = GameStatus::AntiCheat {
            which: crate::model::AntiCheat::BattlEye,
        };
        let inputs = Inputs {
            game: &blocked,
            store: &f.store,
            components: &f.components,
            settings: &f.settings,
        };
        assert_eq!(plan(&inputs).unwrap_err().code(), "anti_cheat_blocked");
    }
}
