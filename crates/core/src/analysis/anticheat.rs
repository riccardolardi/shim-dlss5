//! Kernel anti-cheat detection from folder and file markers, plus a short
//! list of titles whose anti-cheat leaves nothing in the game folder.
//!
//! A hit means "do not inject without the user typing a confirmation", so we
//! lean towards reporting: one marker is enough.

use crate::analysis::walk::Tree;
use crate::model::AntiCheat;

struct Marker {
    which: AntiCheat,
    dirs: &'static [&'static str],
    files: &'static [&'static str],
    /// Matched against the lowercased file name with `ends_with`.
    suffixes: &'static [&'static str],
}

const MARKERS: &[Marker] = &[
    Marker {
        which: AntiCheat::EasyAntiCheat,
        dirs: &["easyanticheat", "easyanticheat_eos"],
        files: &[
            "easyanticheat_x64.dll",
            "easyanticheat_x86.dll",
            "easyanticheat_eos_setup.exe",
            "easyanticheat_setup.exe",
            "start_protected_game.exe",
            "eac_launcher.exe",
        ],
        suffixes: &[],
    },
    Marker {
        which: AntiCheat::BattlEye,
        dirs: &["battleye"],
        files: &[
            "beservice.exe",
            "beservice_x64.exe",
            "beclient_x64.dll",
            "bedaisy.sys",
        ],
        suffixes: &["_be.exe"],
    },
    Marker {
        which: AntiCheat::Vanguard,
        dirs: &[],
        files: &["vgk.sys", "vgc.exe"],
        suffixes: &[],
    },
    Marker {
        which: AntiCheat::Ricochet,
        dirs: &[],
        files: &["randgrid.sys"],
        suffixes: &[],
    },
    Marker {
        which: AntiCheat::GameGuard,
        dirs: &["gameguard"],
        files: &["gamemon.des"],
        suffixes: &[".des"],
    },
    Marker {
        which: AntiCheat::Xigncode,
        dirs: &["xigncode"],
        files: &["x3.xem", "xhunter1.sys"],
        suffixes: &[],
    },
    Marker {
        which: AntiCheat::Javelin,
        dirs: &["eaanticheat"],
        files: &[
            "eaanticheat.gameservicelauncher.exe",
            "eaanticheat.installer.exe",
            "eaanticheat.gameservice.exe",
        ],
        suffixes: &[],
    },
    Marker {
        which: AntiCheat::Ace,
        dirs: &["anticheatexpert", "tersafe"],
        files: &["ace-base.sys", "sguard64.dll", "sguard64.exe", "acesdk.dll"],
        suffixes: &[],
    },
    Marker {
        which: AntiCheat::Mhyprot,
        dirs: &[],
        files: &[
            "mhyprot.sys",
            "mhyprot2.sys",
            "mhyprot3.sys",
            "hoyoprot.sys",
        ],
        suffixes: &[],
    },
];

/// Titles with an anti-cheat that ships outside the game folder.
const TITLES: &[(&str, AntiCheat)] = &[
    ("counter-strike 2", AntiCheat::Vac),
    ("valorant", AntiCheat::Vanguard),
    ("league of legends", AntiCheat::Vanguard),
    ("call of duty", AntiCheat::Ricochet),
    ("overwatch", AntiCheat::Other),
];

pub fn detect(tree: &Tree, title: &str) -> Option<AntiCheat> {
    for m in MARKERS {
        let dir_hit = m.dirs.iter().any(|d| tree.has_dir(d));
        let file_hit = tree.files.iter().any(|f| {
            m.files.contains(&f.name.as_str()) || m.suffixes.iter().any(|s| f.name.ends_with(s))
        });
        if dir_hit || file_hit {
            return Some(m.which);
        }
    }
    let lower = title.to_lowercase();
    TITLES
        .iter()
        .find(|(t, _)| lower.contains(t))
        .map(|(_, which)| *which)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::walk::testing::make_tree;

    fn fixture(files: &[&str]) -> (tempfile::TempDir, Tree) {
        let tmp = tempfile::tempdir().unwrap();
        let entries: Vec<(&str, &[u8])> = files.iter().map(|f| (*f, &b""[..])).collect();
        make_tree(tmp.path(), &entries);
        let t = Tree::collect(tmp.path(), 6);
        (tmp, t)
    }

    #[test]
    fn folder_markers_are_found() {
        let (_t, tree) = fixture(&["Game.exe", "EasyAntiCheat/EasyAntiCheat_EOS_Setup.exe"]);
        assert_eq!(detect(&tree, "Some Game"), Some(AntiCheat::EasyAntiCheat));
        let (_t, tree) = fixture(&["Game.exe", "BattlEye/BEService_x64.exe"]);
        assert_eq!(detect(&tree, "Some Game"), Some(AntiCheat::BattlEye));
    }

    #[test]
    fn file_and_suffix_markers_are_found() {
        let (_t, tree) = fixture(&["bin/Game_BE.exe", "bin/Game.exe"]);
        assert_eq!(detect(&tree, "x"), Some(AntiCheat::BattlEye));
        let (_t, tree) = fixture(&["Game.exe", "GameGuard/npgg.erl", "GameMon.des"]);
        assert_eq!(detect(&tree, "x"), Some(AntiCheat::GameGuard));
        let (_t, tree) = fixture(&["Game.exe", "x3.xem"]);
        assert_eq!(detect(&tree, "x"), Some(AntiCheat::Xigncode));
        let (_t, tree) = fixture(&["Game.exe", "mhyprot2.sys"]);
        assert_eq!(detect(&tree, "x"), Some(AntiCheat::Mhyprot));
    }

    #[test]
    fn title_list_catches_folderless_anti_cheat() {
        let (_t, tree) = fixture(&["cs2.exe"]);
        assert_eq!(detect(&tree, "Counter-Strike 2"), Some(AntiCheat::Vac));
        assert_eq!(detect(&tree, "Call of Duty"), Some(AntiCheat::Ricochet));
        assert_eq!(detect(&tree, "Stardew Valley"), None);
    }
}
