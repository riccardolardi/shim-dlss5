//! End-to-end install against a synthetic game folder with the *real*
//! component payloads: unpack the downloaded assets into a temp store, plan an
//! OptiScaler install and a ReShade install, run both through the journal,
//! verify, uninstall, and prove the folder is byte-identical afterwards.
//!
//! ```powershell
//! cargo run -p shim-win --example e2e_install -- <dir with the downloaded assets>
//! ```
//!
//! Writes only under `%TEMP%\shim-e2e`. Fake user files stand in for the
//! model, the RenoDX add-on and the DLSS runtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use shim_core::{
    components::{ComponentManifest, ComponentStore},
    install::{self, Inputs, Journal},
    model::{Analysis, Bitness, Engine, Game, GameStatus, GraphicsApi, Launcher, Route},
    paths::AppPaths,
    settings::Settings,
};

fn main() {
    let assets = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("pass the directory holding the downloaded assets");
    let root = std::env::temp_dir().join("shim-e2e");
    let _ = std::fs::remove_dir_all(&root);
    let paths = AppPaths::at(root.join("data"));
    paths.ensure().unwrap();

    let store = ComponentStore::new(&paths);
    let components = ComponentManifest::embedded();
    for c in &components.components {
        let alias = match c.id.as_str() {
            "optiscaler" => "optiscaler.7z",
            "reshade" => "reshade_setup.exe",
            _ => "feeder.zip",
        };
        let by_url = assets.join(c.asset.rsplit('/').next().unwrap_or(""));
        let asset = if by_url.is_file() {
            by_url
        } else {
            assets.join(alias)
        };
        store
            .install_from(c, &asset)
            .unwrap_or_else(|e| panic!("{}: {}", c.id, e.detail()));
        println!("component {} {}: {:?}", c.id, c.version, store.status(c));
    }

    let user = root.join("user");
    std::fs::create_dir_all(&user).unwrap();
    for name in ["nvngx_dlssnr.dll", "renodx-dlss5.addon64", "nvngx_dlss.dll"] {
        std::fs::write(user.join(name), format!("fake {name}")).unwrap();
    }
    let settings = Settings {
        model_path: Some(user.join("nvngx_dlssnr.dll")),
        renodx_addon_path: Some(user.join("renodx-dlss5.addon64")),
        dlss_runtime_path: Some(user.join("nvngx_dlss.dll")),
        ..Settings::default()
    };

    for (route, apis, ships) in [
        (Route::OptiScaler, vec![GraphicsApi::Dx12], true),
        (Route::ReShadeRenoDx, vec![GraphicsApi::Dx11], false),
    ] {
        let game_dir = root.join(format!("game-{route:?}"));
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("G.exe"), b"pretend game").unwrap();
        std::fs::write(game_dir.join("OptiScaler.ini"), b"; user's old ini").unwrap();
        let before = snapshot(&game_dir);

        let game = game(&game_dir, apis, route, ships);
        let plan = install::plan(&Inputs {
            game: &game,
            store: &store,
            components: &components,
            settings: &settings,
        })
        .unwrap_or_else(|e| panic!("plan {route:?}: {}", e.detail()));
        println!("\n== {route:?}: {} ops", plan.ops.len());
        for c in plan.describe() {
            println!(
                "   {:?} {} — {}",
                c.kind,
                c.path.strip_prefix(&game_dir).unwrap().display(),
                c.note
            );
        }

        let manifest = Journal::run(&paths, &game.id, &plan, false, &mut |s| {
            println!("   [{}/{}] {}", s.index + 1, s.total, s.message);
        })
        .unwrap_or_else(|e| panic!("install {route:?}: {}", e.detail()));
        assert!(game_dir.join("shim.json").is_file());
        assert!(game_dir.join("nvngx_dlssnr.dll").is_file());
        let after = snapshot(&game_dir);
        assert!(after.len() > before.len(), "files were added");
        println!(
            "   installed: {} files recorded, {} in folder",
            manifest.files.len(),
            after.len()
        );

        install::uninstall(&paths, &manifest)
            .unwrap_or_else(|e| panic!("uninstall {route:?}: {}", e.detail()));
        let restored = snapshot(&game_dir);
        assert_eq!(
            restored, before,
            "{route:?}: folder is not byte-identical after uninstall"
        );
        println!(
            "   uninstalled: folder byte-identical ({} files)",
            restored.len()
        );
    }
    println!("\nOK — everything under {}", root.display());
}

fn game(dir: &Path, apis: Vec<GraphicsApi>, route: Route, ships_dlss: bool) -> Game {
    Game {
        id: format!("e2e-{route:?}"),
        launcher: Launcher::Custom,
        title: "E2E".into(),
        install_dir: dir.to_path_buf(),
        launcher_id: None,
        analysis: Some(Analysis {
            exe: dir.join("G.exe"),
            exe_size: 1,
            exe_mtime: 1,
            bitness: Bitness::X64,
            apis,
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

fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
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
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}
