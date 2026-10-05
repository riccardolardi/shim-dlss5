//! Opt-in result reports. After a game has run, the user may send one small,
//! anonymous record of what happened, which the website aggregates per game.
//! Nothing is sent without the user seeing the exact payload first; there is
//! no user or machine id, no file path, and no model hash in it.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    api::LastRun,
    install::InstallManifest,
    model::{Game, Launcher, Route},
    platform::{Hive, Registry},
    Error, Result,
};

pub const REPORT_URL: &str = "https://shim-dlss5.vercel.app/api/report";
pub const REPORT_SCHEMA: u32 = 1;
const MAX_LOG_LINES: usize = 5;
const MAX_LOG_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Outcome {
    /// The DLSS 5 pass ran and the game was playable.
    Works,
    /// The game ran, but the pass had no visible effect.
    NoEffect,
    /// The game crashed or would not start.
    Crashes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Gpu {
    pub name: String,
    #[ts(type = "number | null")]
    pub vram_mb: Option<u64>,
    /// NVIDIA's own numbering (e.g. `616.56`) when it can be derived.
    pub driver: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Report {
    pub schema: u32,
    pub app_version: String,
    pub launcher: Launcher,
    /// Launcher-native id (e.g. the Steam app id), used to group reports.
    pub launcher_id: Option<String>,
    pub title: String,
    pub route: Route,
    /// Only for the OptiScaler + DLSS 5 route.
    pub before_upscale: Option<bool>,
    /// `(component id, version)` as installed.
    pub components: Vec<(String, String)>,
    pub game_dlss: Option<String>,
    pub gpu: Option<Gpu>,
    /// Whether the user's model carries an intact NVIDIA signature.
    pub model_signed: Option<bool>,
    pub outcome: Outcome,
    /// Up to five decisive log lines, with file paths cut down to file names.
    pub log: Vec<String>,
}

pub struct Inputs<'a> {
    pub game: &'a Game,
    pub manifest: &'a InstallManifest,
    pub gpu: Option<Gpu>,
    pub model_signed: Option<bool>,
    pub last_run: Option<&'a LastRun>,
    pub outcome: Outcome,
    pub app_version: &'a str,
}

pub fn build(i: &Inputs<'_>) -> Report {
    let before_upscale = match i.manifest.route {
        Route::OptiScalerDlssNr => Some(i.game.neural.unwrap_or_default().before_upscale),
        _ => None,
    };
    let log = i
        .last_run
        .map(|r| {
            r.lines
                .iter()
                .rev()
                .take(MAX_LOG_LINES)
                .rev()
                .map(|l| truncate(&strip_paths(l), MAX_LOG_CHARS))
                .collect()
        })
        .unwrap_or_default();
    Report {
        schema: REPORT_SCHEMA,
        app_version: i.app_version.to_string(),
        launcher: i.game.launcher,
        launcher_id: i.game.launcher_id.clone(),
        title: i.game.title.clone(),
        route: i.manifest.route,
        before_upscale,
        components: i
            .manifest
            .components
            .iter()
            .map(|c| (c.id.clone(), c.version.clone()))
            .collect(),
        game_dlss: i
            .game
            .analysis
            .as_ref()
            .and_then(|a| a.dlss_version.clone()),
        gpu: i.gpu.clone(),
        model_signed: i.model_signed,
        outcome: i.outcome,
        log,
    }
}

/// What the app pre-selects in the form; the user always decides.
pub fn suggested_outcome(last_run: Option<&LastRun>) -> Outcome {
    match last_run {
        Some(r) if r.failed => Outcome::Crashes,
        _ => Outcome::Works,
    }
}

/// POST the report. The server answers 2xx or explains why not.
pub fn send(report: &Report) -> Result<()> {
    let err = |detail: String| Error::Download {
        url: REPORT_URL.to_string(),
        detail,
    };
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("shim-dlss5/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| err(e.to_string()))?;
    let response = client
        .post(REPORT_URL)
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(report).map_err(|e| err(e.to_string()))?)
        .send()
        .map_err(|e| err(e.to_string()))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body = response.text().unwrap_or_default();
    Err(err(format!(
        "server answered {status}: {}",
        truncate(&body, 300)
    )))
}

const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// The display adapter from the registry, preferring an NVIDIA one.
pub fn detect_gpu(registry: &dyn Registry) -> Option<Gpu> {
    let mut found: Vec<Gpu> = registry
        .subkeys(Hive::LocalMachine, DISPLAY_CLASS)
        .into_iter()
        .filter(|k| k.len() == 4 && k.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|k| {
            let key = format!(r"{DISPLAY_CLASS}\{k}");
            let name = registry.read_string(Hive::LocalMachine, &key, "DriverDesc")?;
            let driver = registry.read_string(Hive::LocalMachine, &key, "DriverVersion");
            let vram = registry
                .read_u64(Hive::LocalMachine, &key, "HardwareInformation.qwMemorySize")
                .map(|b| b / (1024 * 1024));
            let nvidia = name.to_lowercase().contains("nvidia");
            Some(Gpu {
                driver: driver.map(|d| {
                    if nvidia {
                        nvidia_driver(&d).unwrap_or(d)
                    } else {
                        d
                    }
                }),
                vram_mb: vram,
                name,
            })
        })
        .collect();
    found.sort_by_key(|g| !g.name.to_lowercase().contains("nvidia"));
    found.into_iter().next()
}

/// Windows driver version `32.0.16.1656` → NVIDIA's `616.56`: the last five
/// digits of the last two fields.
pub fn nvidia_driver(windows: &str) -> Option<String> {
    let parts: Vec<&str> = windows.split('.').collect();
    let [.., a, b] = parts.as_slice() else {
        return None;
    };
    let digits = format!("{a}{b:0>4}");
    if digits.len() < 5 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let tail = &digits[digits.len() - 5..];
    Some(format!("{}.{}", &tail[..3], &tail[3..]))
}

/// Replace Windows paths (`C:\Users\x\…\file.dll`, `D:/games/…`) with just
/// the file name, so a log line can't reveal a user name or folder layout.
pub fn strip_paths(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        let is_drive = i + 2 < chars.len()
            && chars[i].is_ascii_alphabetic()
            && chars[i + 1] == ':'
            && (chars[i + 2] == '\\' || chars[i + 2] == '/')
            && (i == 0 || !chars[i - 1].is_ascii_alphanumeric());
        if !is_drive {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !matches!(chars[i], '\'' | '"' | ',' | '\t' | '\n') {
            // A space ends the path unless the next segment still looks like part of it.
            if chars[i] == ' ' {
                let rest: String = chars[i + 1..]
                    .iter()
                    .take_while(|c| !matches!(c, '\'' | '"' | ',' | '\t' | '\n'))
                    .collect();
                if !(rest.contains('\\') || rest.contains('/')) {
                    break;
                }
            }
            i += 1;
        }
        // A closing bracket right after the file name belongs to the sentence.
        // A closing bracket at the end belongs to the sentence unless the
        // path opened it, as in "Program Files (x86)".
        while i > start && matches!(chars[i - 1], ')' | ']') {
            let open = if chars[i - 1] == ')' { '(' } else { '[' };
            let inside = &chars[start..i - 1];
            let opens = inside.iter().filter(|c| **c == open).count();
            let closes = inside.iter().filter(|c| **c == chars[i - 1]).count();
            if opens > closes {
                break;
            }
            i -= 1;
        }
        let path: String = chars[start..i].iter().collect();
        let name = path
            .trim_end_matches(['\\', '/'])
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("");
        out.push_str("…\\");
        out.push_str(name);
    }
    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n - 1).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::manifest::{ComponentPin, INSTALL_SCHEMA};
    use crate::model::{GameStatus, NeuralOptions};
    use crate::platform::testing::FakeRegistry;
    use std::path::PathBuf;

    #[test]
    fn nvidia_driver_numbering() {
        assert_eq!(nvidia_driver("32.0.16.1656").as_deref(), Some("616.56"));
        assert_eq!(nvidia_driver("32.0.15.7314").as_deref(), Some("573.14"));
        assert_eq!(nvidia_driver("31.0.15.0123").as_deref(), Some("501.23"));
        assert_eq!(nvidia_driver("garbage"), None);
    }

    #[test]
    fn paths_are_cut_to_file_names() {
        assert_eq!(
            strip_paths(
                r"found at C:\Users\ricca\AppData\Local\shim\user-files\nvngx_dlssnr.dll [ok]"
            ),
            r"found at …\nvngx_dlssnr.dll [ok]"
        );
        assert_eq!(
            strip_paths(
                r"Loading add-on from 'C:\Program Files (x86)\Steam\steamapps\common\MSFS2024\renodx-dlss5.addon64' ..."
            ),
            r"Loading add-on from '…\renodx-dlss5.addon64' ..."
        );
        assert_eq!(
            strip_paths("no paths here: 3:4 ratio"),
            "no paths here: 3:4 ratio"
        );
        assert_eq!(
            strip_paths("D:/games/x/game.exe started"),
            r"…\game.exe started"
        );
        assert_eq!(strip_paths(r"(see C:\a\b.log)"), r"(see …\b.log)");
    }

    #[test]
    fn gpu_prefers_nvidia_and_reads_vram() {
        let mut reg = FakeRegistry::default();
        reg.add(
            Hive::LocalMachine,
            &format!(r"{DISPLAY_CLASS}\0000"),
            &[
                ("DriverDesc", "Intel(R) UHD Graphics"),
                ("DriverVersion", "31.0.101.5186"),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            &format!(r"{DISPLAY_CLASS}\0001"),
            &[
                ("DriverDesc", "NVIDIA GeForce RTX 5070"),
                ("DriverVersion", "32.0.16.1656"),
                ("HardwareInformation.qwMemorySize", "12820938752"),
            ],
        );
        reg.add(
            Hive::LocalMachine,
            &format!(r"{DISPLAY_CLASS}\Properties"),
            &[("x", "y")],
        );
        let gpu = detect_gpu(&reg).unwrap();
        assert_eq!(gpu.name, "NVIDIA GeForce RTX 5070");
        assert_eq!(gpu.driver.as_deref(), Some("616.56"));
        assert_eq!(gpu.vram_mb, Some(12227));
        assert_eq!(detect_gpu(&FakeRegistry::default()), None);
    }

    #[test]
    fn report_carries_route_placement_versions_and_trimmed_log() {
        let game = Game {
            id: "g".into(),
            launcher: Launcher::Steam,
            title: "Bright Memory".into(),
            install_dir: PathBuf::from("C:/g"),
            launcher_id: Some("1178830".into()),
            analysis: None,
            status: GameStatus::Installed {
                route: Route::OptiScalerDlssNr,
            },
            cover: None,
            hidden: false,
            mode: None,
            neural: Some(NeuralOptions {
                before_upscale: false,
            }),
        };
        let manifest = InstallManifest {
            schema: INSTALL_SCHEMA,
            game_id: "g".into(),
            exe: PathBuf::from("C:/g/G.exe"),
            route: Route::OptiScalerDlssNr,
            installed_at: 1,
            components: vec![ComponentPin {
                id: "optiscaler-dlssnr".into(),
                version: "0.2.0".into(),
                sha256: "x".into(),
            }],
            model_sha256: Some("secret".into()),
            user_files: vec![],
            anti_cheat_override: false,
            files: vec![],
            side_effects: vec![],
        };
        let run = LastRun {
            log: "x".into(),
            modified: 1,
            lines: (1..=8)
                .map(|n| format!(r"line {n} C:\Users\ricca\x\f{n}.dll"))
                .collect(),
            failed: true,
        };
        let r = build(&Inputs {
            game: &game,
            manifest: &manifest,
            gpu: None,
            model_signed: Some(false),
            last_run: Some(&run),
            outcome: suggested_outcome(Some(&run)),
            app_version: "0.1.4",
        });
        assert_eq!(r.before_upscale, Some(false));
        assert_eq!(
            r.components,
            vec![("optiscaler-dlssnr".into(), "0.2.0".into())]
        );
        assert_eq!(r.outcome, Outcome::Crashes);
        assert_eq!(r.log.len(), 5);
        assert_eq!(r.log[0], r"line 4 …\f4.dll");
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("ricca") && !json.contains("secret"),
            "{json}"
        );
    }
}
