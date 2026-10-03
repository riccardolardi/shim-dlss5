//! Which route a game gets, and the one sentence that explains it.
//!
//! The decision tree is PLAN §5.4. It is a pure function of the analysis so
//! the whole table is unit-tested without touching a disk.

use crate::model::{Analysis, Bitness, GameStatus, GraphicsApi, Route};

/// Turn an analysis into the card status. Install state (Installed /
/// UpdateAvailable) is layered on later by the install manifest; this only
/// answers "could we, and how".
pub fn decide(a: &Analysis) -> GameStatus {
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
        (Some(api), _, true) => ready(
            Route::OptiScaler,
            format!("{api} game that ships DLSS, so OptiScaler takes over the DLSS slot."),
        ),
        (Some(api), _, false) => ready(
            Route::ReShadeRenoDx,
            format!("{api} game without DLSS, so ReShade adds the RenoDX DLSS 5 pass."),
        ),
        (None, true, true) => ready(
            Route::OptiScaler,
            "Vulkan game that ships DLSS, so OptiScaler takes over the DLSS slot.".into(),
        ),
        // ReShade's Vulkan path is a machine-wide layer registered in the
        // registry, which a per-game file journal cannot undo cleanly.
        (None, true, false) => GameStatus::Unsupported {
            reason: "Vulkan game without DLSS (arrives in Phase 3)".into(),
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
            decide(&a),
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
            matches!(decide(&a), GameStatus::Unsupported { reason } if reason.contains("32-bit"))
        );
    }

    #[test]
    fn routing_table() {
        use GraphicsApi::*;
        let cases: &[(&[GraphicsApi], bool, Option<Route>)] = &[
            (&[Dx12], true, Some(Route::OptiScaler)),
            (&[Dx11], true, Some(Route::OptiScaler)),
            (&[Dx12, Vulkan], false, Some(Route::ReShadeRenoDx)),
            (&[Dx11], false, Some(Route::ReShadeRenoDx)),
            (&[Vulkan], true, Some(Route::OptiScaler)),
            (&[Vulkan], false, None),
            (&[Dx9], false, None),
            (&[Dx10], true, None),
            (&[OpenGl], false, None),
            (&[], false, None),
        ];
        for (apis, dlss, want) in cases {
            let got = decide(&analysis(apis, *dlss));
            assert_eq!(route_of(&got), *want, "{apis:?} dlss={dlss}: {got:?}");
        }
    }

    #[test]
    fn reason_names_the_api() {
        match decide(&analysis(&[GraphicsApi::Dx12, GraphicsApi::Dx11], false)) {
            GameStatus::Ready { reason, .. } => assert!(reason.starts_with("DX12")),
            other => panic!("{other:?}"),
        }
        match decide(&analysis(&[GraphicsApi::Dx9], false)) {
            GameStatus::Unsupported { reason } => assert_eq!(reason, "DirectX 9 game"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn foreign_tools_are_refused_not_clobbered() {
        let mut a = analysis(&[GraphicsApi::Dx12], false);
        a.foreign_reshade = true;
        assert!(
            matches!(decide(&a), GameStatus::Unsupported { reason } if reason.contains("ReShade"))
        );
        a.foreign_optiscaler = true;
        assert!(
            matches!(decide(&a), GameStatus::Unsupported { reason } if reason.contains("OptiScaler"))
        );
    }
}
