//! Which route a game gets, and the one sentence that explains it.
//!
//! The decision tree is PLAN §5.4. It is a pure function of the analysis and
//! the user's per-game mode, so the whole table is unit-tested without
//! touching a disk.
//!
//! Games that ship DLSS default to ReShade hosting the RenoDX DLSS 5 add-on
//! over the game's own DLSS output. The user may instead pick the OptiScaler
//! DLSS-NR fork (upscaler + neural pass in one DLL) or plain OptiScaler (no
//! neural pass). Games without DLSS always get ReShade + DLSS5-Feeder +
//! RenoDX. (shim 0.1.0 routed DLSS games to upstream OptiScaler, which does
//! not load the model at all.)

use crate::model::{Analysis, Bitness, GameStatus, GraphicsApi, InstallMode, Route};

/// Turn an analysis and the user's mode into the card status. Install state
/// (Installed / UpdateAvailable) is layered on later by the install
/// manifest; this only answers "could we, and how".
pub fn decide(a: &Analysis, mode: Option<InstallMode>) -> GameStatus {
    if let Some(which) = a.anti_cheat {
        return GameStatus::AntiCheat { which };
    }
    if a.bitness == Bitness::X86 {
        return GameStatus::Unsupported {
            reason: "32-bit game".to_string(),
        };
    }
    if a.foreign_optiscaler || a.foreign_reshade {
        let what = if a.foreign_optiscaler {
            "OptiScaler"
        } else {
            "ReShade"
        };
        return GameStatus::Unsupported {
            reason: format!("{what} is already installed by something else"),
        };
    }

    let has = |api| a.apis.contains(&api);
    let dx = if has(GraphicsApi::Dx12) {
        Some("DX12")
    } else if has(GraphicsApi::Dx11) {
        Some("DX11")
    } else {
        None
    };

    match (dx, has(GraphicsApi::Vulkan), a.ships_dlss) {
        (Some(api), _, true) => dlss_game(api, mode.unwrap_or_default()),
        (Some(api), _, false) => ready(
            Route::ReShadeFeeder,
            format!(
                "{api} game without DLSS: ReShade loads DLSS5-Feeder to supply depth and \
                 motion vectors, and the RenoDX DLSS 5 add-on renders from them."
            ),
        ),
        // ReShade's Vulkan path is a machine-wide layer registered in the
        // registry, which a per-game file journal cannot undo cleanly.
        (None, true, _) => GameStatus::Unsupported {
            reason: "Vulkan game (arrives in Phase 3)".into(),
        },
        (None, false, _) => GameStatus::Unsupported {
            reason: match a.apis.first() {
                Some(GraphicsApi::Dx9) => "DirectX 9 game".into(),
                Some(GraphicsApi::Dx10) => "DirectX 10 game".into(),
                Some(GraphicsApi::OpenGl) => "OpenGL game".into(),
                _ => "no supported graphics API found".into(),
            },
        },
    }
}

fn dlss_game(api: &str, mode: InstallMode) -> GameStatus {
    match mode {
        InstallMode::Dlss5 => ready(
            Route::ReShadeRenoDx,
            format!(
                "{api} game that ships DLSS: ReShade loads the RenoDX DLSS 5 add-on, which \
                 takes over the game's own DLSS pass. Turn DLSS on in the game's settings."
            ),
        ),
        InstallMode::OptiScalerDlss5 => ready(
            Route::OptiScalerDlssNr,
            format!(
                "{api} game that ships DLSS: the OptiScaler DLSS-NR fork replaces the DLSS \
                 slot, upscales, then runs the DLSS 5 model on its own output. Turn DLSS on \
                 in the game's settings; enable the pass in OptiScaler's overlay if it is off."
            ),
        ),
        InstallMode::OptiScalerOnly => ready(
            Route::OptiScaler,
            format!(
                "{api} game that ships DLSS: upstream OptiScaler replaces the DLSS slot with \
                 its own upscaler. No DLSS 5 pass; the model is not used."
            ),
        ),
    }
}

/// Whether the mode choice is offered for this game at all.
pub fn mode_applies(a: &Analysis) -> bool {
    a.ships_dlss && (a.apis.contains(&GraphicsApi::Dx12) || a.apis.contains(&GraphicsApi::Dx11))
}

fn ready(route: Route, reason: String) -> GameStatus {
    GameStatus::Ready { route, reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AntiCheat, Engine};
    use std::path::PathBuf;

    fn analysis(apis: &[GraphicsApi], ships_dlss: bool) -> Analysis {
        Analysis {
            exe: PathBuf::from("g.exe"),
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
        }
    }

    fn route_of(s: &GameStatus) -> Option<Route> {
        match s {
            GameStatus::Ready { route, .. } => Some(*route),
            _ => None,
        }
    }

    #[test]
    fn anti_cheat_blocks_everything_else() {
        let mut a = analysis(&[GraphicsApi::Dx12], true);
        a.anti_cheat = Some(AntiCheat::BattlEye);
        assert_eq!(
            decide(&a, None),
            GameStatus::AntiCheat {
                which: AntiCheat::BattlEye
            }
        );
    }

    #[test]
    fn thirty_two_bit_is_unsupported_in_v1() {
        let mut a = analysis(&[GraphicsApi::Dx11], false);
        a.bitness = Bitness::X86;
        assert!(
            matches!(decide(&a, None), GameStatus::Unsupported { reason } if reason.contains("32-bit"))
        );
    }

    #[test]
    fn routing_table_with_default_mode() {
        use GraphicsApi::*;
        let cases: &[(&[GraphicsApi], bool, Option<Route>)] = &[
            (&[Dx12], true, Some(Route::ReShadeRenoDx)),
            (&[Dx11], true, Some(Route::ReShadeRenoDx)),
            (&[Dx12, Vulkan], false, Some(Route::ReShadeFeeder)),
            (&[Dx11], false, Some(Route::ReShadeFeeder)),
            (&[Vulkan], true, None),
            (&[Vulkan], false, None),
            (&[Dx9], false, None),
            (&[Dx10], true, None),
            (&[OpenGl], false, None),
            (&[], false, None),
        ];
        for (apis, dlss, want) in cases {
            let got = decide(&analysis(apis, *dlss), None);
            assert_eq!(route_of(&got), *want, "{apis:?} dlss={dlss}: {got:?}");
        }
    }

    #[test]
    fn mode_picks_the_route_for_dlss_games_only() {
        let dlss = analysis(&[GraphicsApi::Dx12], true);
        assert_eq!(
            route_of(&decide(&dlss, Some(InstallMode::Dlss5))),
            Some(Route::ReShadeRenoDx)
        );
        assert_eq!(
            route_of(&decide(&dlss, Some(InstallMode::OptiScalerDlss5))),
            Some(Route::OptiScalerDlssNr)
        );
        assert_eq!(
            route_of(&decide(&dlss, Some(InstallMode::OptiScalerOnly))),
            Some(Route::OptiScaler)
        );
        assert!(mode_applies(&dlss));

        let no_dlss = analysis(&[GraphicsApi::Dx12], false);
        for mode in [InstallMode::OptiScalerDlss5, InstallMode::OptiScalerOnly] {
            assert_eq!(
                route_of(&decide(&no_dlss, Some(mode))),
                Some(Route::ReShadeFeeder)
            );
        }
        assert!(!mode_applies(&no_dlss));
        assert!(!mode_applies(&analysis(&[GraphicsApi::Vulkan], true)));
    }

    #[test]
    fn reason_names_the_api_and_tells_the_user_what_to_do() {
        match decide(
            &analysis(&[GraphicsApi::Dx12, GraphicsApi::Dx11], true),
            None,
        ) {
            GameStatus::Ready { reason, .. } => {
                assert!(reason.starts_with("DX12"));
                assert!(reason.contains("Turn DLSS on"));
            }
            other => panic!("{other:?}"),
        }
        match decide(&analysis(&[GraphicsApi::Dx9], false), None) {
            GameStatus::Unsupported { reason } => assert_eq!(reason, "DirectX 9 game"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn foreign_tools_are_refused_not_clobbered() {
        let mut a = analysis(&[GraphicsApi::Dx12], false);
        a.foreign_reshade = true;
        assert!(
            matches!(decide(&a, None), GameStatus::Unsupported { reason } if reason.contains("ReShade"))
        );
        a.foreign_optiscaler = true;
        assert!(
            matches!(decide(&a, None), GameStatus::Unsupported { reason } if reason.contains("OptiScaler"))
        );
    }
}
